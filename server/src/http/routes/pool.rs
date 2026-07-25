//! `GET /api/pool/{version}/manifest`.
//!
//! Served for **every** version for as long as the service runs: a trial recorded under v1 stays
//! verifiable only while v1's manifest still answers (D5). The manifest is half the derivation —
//! the draw produces indices, and the manifest is what those indices point at — so without this
//! endpoint a published trial is a row of hashes nobody can recompute anything from.
//!
//! # Where an older manifest comes from
//!
//! A process serves one pool (`--pool`), the version it draws new trials under. Older versions are
//! read from **the directory that file sits in, as `v<N>.json`** — the name `poolctl build` writes
//! by default, so archiving a version is a copy and nothing else. Deliberately not a second flag:
//! an endpoint whose promise is "every version, forever" should not depend on an operator having
//! remembered to configure a second path, because the day they forget nothing looks wrong until a
//! verifier asks.
//!
//! Bytes are served verbatim from that file rather than re-serialised from a parsed manifest. A
//! verifier hashes what they downloaded, so re-serialising would publish one byte sequence and
//! hand out another — and would drop `created` and `count`, which the contract carries for readers
//! and the derivation does not use.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::http::{ApiError, AppState};
use crate::pool::Manifest;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/pool/{version}/manifest", get(manifest))
}

/// A published pool manifest, by version.
async fn manifest(
    State(state): State<AppState>,
    UrlPath(version): UrlPath<u32>,
) -> Result<Response, ApiError> {
    let serving = state.pool.version;

    for path in candidates(&state.config.pool_path, version, serving) {
        match published(&path, version) {
            Ok(Some((raw, found))) => {
                // The one cross-check worth paying for: for the version this process draws under,
                // the file on disk must still be the manifest in memory. They part company when a
                // manifest is edited after startup — and then every trial committed since was
                // drawn against something other than what a verifier downloads, which surfaces as
                // a verification failure blamed on the trial rather than on the swap.
                if version == serving && found.manifest_hash != state.pool.manifest_hash {
                    tracing::error!(
                        version,
                        path = %path.display(),
                        on_disk = %found.manifest_hash,
                        in_memory = %state.pool.manifest_hash,
                        "the pool manifest on disk is no longer the one trials are drawn under"
                    );
                    return Err(ApiError::Internal);
                }
                return Ok(served(raw, CACHE_FOREVER));
            }
            Ok(None) => {}
            Err(why) => {
                // Refused rather than skipped. Falling through to the next candidate would answer
                // a request for one version out of another version's file, and the caller has no
                // way to tell.
                tracing::error!(
                    version,
                    path = %path.display(),
                    why,
                    "a published pool manifest is unusable"
                );
                return Err(ApiError::Internal);
            }
        }
    }

    if version == serving {
        // Nothing on disk, but this process holds the manifest it validated at startup, so the one
        // version currently producing trials can still be recomputed. `created` and `count` are
        // absent — they exist only in the published file — while every field the derivation needs
        // is here. Not cached: the file may be restored, and the full form should win the moment
        // it is.
        tracing::warn!(
            version,
            path = %state.config.pool_path.display(),
            "serving the pool manifest from memory; the published file is missing"
        );
        let raw = serde_json::to_string(&*state.pool).map_err(|e| {
            tracing::error!(error = %e, "the in-memory pool manifest will not serialise");
            ApiError::Internal
        })?;
        return Ok(served(raw, "no-store"));
    }

    if version < serving {
        // A version this service has already moved past, with no file to answer from. D5's promise
        // is broken until an operator puts the file back, so it is a 500 and a loud line — not a
        // 404, which would tell a verifier holding a v1 trial that v1 never existed and send them
        // looking for their own mistake.
        tracing::error!(
            version,
            serving,
            dir = %archive_dir(&state.config.pool_path).display(),
            "an earlier pool version has no manifest to serve; trials recorded under it cannot \
             be verified"
        );
        return Err(ApiError::Internal);
    }

    Err(ApiError::NotFound)
}

/// A published version is never edited — `poolctl build` refuses to overwrite one, because trials
/// recorded under it must stay verifiable. So this is the rare response that really can be fetched
/// once and kept.
const CACHE_FOREVER: &str = "public, max-age=31536000, immutable";

fn served(raw: String, cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, cache),
        ],
        raw,
    )
        .into_response()
}

/// Where the archive lives: beside the manifest this process was started with.
fn archive_dir(pool_path: &Path) -> &Path {
    // A bare `--pool manifest.json` has no parent component, and that means the working directory.
    pool_path.parent().unwrap_or(Path::new("."))
}

/// Files that might hold `version`, in the order to try them.
///
/// The configured path goes first for the version being served, because that is the file this
/// process actually loaded. Preferring an archived copy would let a stale `v3.json` sitting beside
/// it answer for the pool trials are really drawn from.
fn candidates(pool_path: &Path, version: u32, serving: u32) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(2);
    if version == serving {
        paths.push(pool_path.to_path_buf());
    }
    paths.push(archive_dir(pool_path).join(format!("v{version}.json")));
    paths
}

/// Reads a published manifest file: `Ok(None)` when there is no such file, `Err` when there is one
/// and it cannot be trusted.
///
/// Everything the server checks at startup is checked again here — sorted ids, declared
/// categories, the hash agreeing with the pairs — because an archived manifest is read by nobody
/// until a verifier asks for it, and by then the trials it describes are years old. Serving one
/// that does not validate produces a verification failure that looks like a falsified trial.
fn published(path: &Path, version: u32) -> Result<Option<(String, Manifest)>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    let manifest: Manifest = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    manifest.validate()?;

    // The one error the hash cannot catch: a file that answers for one version while declaring
    // another is internally consistent, and the verifier recomputes the trial against the wrong
    // pool entirely.
    if manifest.version != version {
        return Err(format!(
            "the file declares version {} but was asked for {version}",
            manifest.version
        ));
    }
    Ok(Some((raw, manifest)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::http::{router, test_support};
    use crate::pool::ImageEntry;

    /// A directory under the temp directory, removed with its contents when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut p = std::env::temp_dir();
            p.push(format!("vriltrainer-pool-{tag}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn manifest_of(version: u32, ids: &[&str]) -> Manifest {
        let categories = vec!["a".to_string(), "b".to_string()];
        let images: Vec<ImageEntry> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| ImageEntry {
                id: (*id).into(),
                category: categories[i % 2].clone(),
            })
            .collect();
        Manifest {
            version,
            manifest_hash: Manifest::compute_hash(&categories, &images),
            categories,
            images,
        }
    }

    /// State serving `pool`, with `--pool` pointing into `dir`.
    fn state_in(dir: &TempDir, pool: Manifest) -> AppState {
        let config = Config {
            pool_path: dir.0.join("manifest.json"),
            ..Config::default()
        };
        AppState {
            config: Arc::new(config),
            pool: Arc::new(pool),
            ..test_support::state()
        }
    }

    fn write(path: &Path, manifest: &Manifest) {
        std::fs::write(path, serde_json::to_string(manifest).unwrap()).unwrap();
    }

    async fn get(state: &AppState, uri: &str) -> Response {
        let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
        router(state.clone()).oneshot(request).await.unwrap()
    }

    async fn json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// D5's promise, which is the whole reason this endpoint exists: a trial recorded under v1
    /// stays verifiable after the site has moved on to v2.
    #[tokio::test]
    async fn an_older_version_still_answers_after_the_pool_has_moved_on() {
        let dir = TempDir::new("older");
        let v1 = manifest_of(1, &["img_1", "img_2"]);
        let v2 = manifest_of(2, &["img_1", "img_2", "img_3"]);
        write(&dir.0.join("v1.json"), &v1);
        write(&dir.0.join("manifest.json"), &v2);

        let state = state_in(&dir, v2.clone());

        let response = get(&state, "/api/pool/1/manifest").await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["version"], 1);
        assert_eq!(
            body["manifest_hash"], v1.manifest_hash,
            "an old trial recomputes against the pool it was drawn under, not the current one"
        );

        let body = json(get(&state, "/api/pool/2/manifest").await).await;
        assert_eq!(body["manifest_hash"], v2.manifest_hash);
    }

    /// The published file is served byte for byte, so what a verifier hashes is what the operator
    /// published — including the fields the derivation does not use.
    #[tokio::test]
    async fn the_published_bytes_are_what_is_served() {
        let dir = TempDir::new("verbatim");
        let v1 = manifest_of(1, &["img_1", "img_2"]);
        // What `poolctl build` writes: the manifest plus `created` and `count`, which the server's
        // own type does not carry.
        let mut published = serde_json::to_value(&v1).unwrap();
        published["created"] = serde_json::json!("2026-07-20T09:14:00Z");
        published["count"] = serde_json::json!(2);
        let raw = serde_json::to_string_pretty(&published).unwrap();
        std::fs::write(dir.0.join("v1.json"), &raw).unwrap();

        let state = state_in(&dir, manifest_of(2, &["img_1", "img_2", "img_3"]));
        let response = get(&state, "/api/pool/1/manifest").await;
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(String::from_utf8(bytes.to_vec()).unwrap(), raw);
    }

    /// A version this service has passed with nothing to answer from is an operator fault and has
    /// to read as one. A 404 would tell a verifier holding a v1 trial that v1 never existed.
    #[tokio::test]
    async fn a_missing_earlier_version_is_a_fault_and_not_a_404() {
        let dir = TempDir::new("missing");
        let v3 = manifest_of(3, &["img_1", "img_2"]);
        write(&dir.0.join("manifest.json"), &v3);
        let state = state_in(&dir, v3);

        assert_eq!(
            get(&state, "/api/pool/1/manifest").await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        // A version that has not been cut yet genuinely does not exist.
        assert_eq!(
            get(&state, "/api/pool/9/manifest").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    /// The version currently drawing trials answers even with no file at all, because it is the
    /// one version this process is certain of.
    #[tokio::test]
    async fn the_served_version_answers_from_memory_when_the_file_is_gone() {
        let dir = TempDir::new("memory");
        let v1 = manifest_of(1, &["img_1", "img_2"]);
        let state = state_in(&dir, v1.clone());

        let response = get(&state, "/api/pool/1/manifest").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store",
            "the reduced form must not outlive the file's restoration"
        );
        assert_eq!(json(response).await["manifest_hash"], v1.manifest_hash);
    }

    /// The failure the cross-check exists for: the file was swapped under the running process, so
    /// trials committed since were drawn against something a verifier would never see.
    #[tokio::test]
    async fn a_manifest_edited_under_the_running_process_is_refused() {
        let dir = TempDir::new("swapped");
        let serving = manifest_of(1, &["img_1", "img_2"]);
        write(&dir.0.join("manifest.json"), &manifest_of(1, &["img_7"]));
        let state = state_in(&dir, serving);

        assert_eq!(
            get(&state, "/api/pool/1/manifest").await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// Internally consistent and still wrong: a file whose declared version is not the one asked
    /// for would have a verifier recompute a trial against the wrong pool.
    #[tokio::test]
    async fn a_file_that_declares_another_version_is_refused() {
        let dir = TempDir::new("mislabelled");
        write(&dir.0.join("v1.json"), &manifest_of(2, &["img_1", "img_2"]));
        let state = state_in(&dir, manifest_of(3, &["img_1", "img_2"]));

        assert_eq!(
            get(&state, "/api/pool/1/manifest").await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn the_served_version_prefers_the_file_the_process_was_started_with() {
        let pool = PathBuf::from("/srv/pool/manifest.json");
        assert_eq!(
            candidates(&pool, 3, 3),
            vec![pool.clone(), PathBuf::from("/srv/pool/v3.json")]
        );
        assert_eq!(
            candidates(&pool, 1, 3),
            vec![PathBuf::from("/srv/pool/v1.json")]
        );
    }
}
