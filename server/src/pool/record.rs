//! Writing the served pool into `pool_version` and `pool_image`.
//!
//! Those two tables were in the schema from the start and nothing ever wrote them, which made the
//! database silent about the one thing a verifier has to know and cannot get from the log alone:
//! which images a pool version consisted of. The manifests are served from disk (see
//! [`crate::http::routes::pool`]), so an operator who loses `v1.json` loses the ability to
//! recompute every trial recorded under v1 — while the backup, which is the artefact the whole
//! project is built to preserve, carried nothing about it.
//!
//! Recorded at startup, once, by whichever process gets there first. Both processes serve the same
//! pool against the same file (D24), so the second finds its own row already written.
//!
//! **A re-cut version is a warning, not a refusal.** D34 makes the version number a pointer and
//! puts the binding hash in each trial's commit entry precisely so a version *can* be re-cut, so
//! refusing to start would contradict the decision that makes re-cutting safe. What it must not be
//! is silent: a re-cut leaves every trial committed under the old hash verifiable only against a
//! manifest this service no longer serves, and that is worth a line an operator can find.

use rusqlite::params;

use crate::db::{Db, DbError};
use crate::pool::Manifest;

/// Records the manifest this process serves, and says whether the version had been seen before
/// pointing somewhere else.
pub fn served(db: &Db, pool: &Manifest, now: &str) -> Result<Recorded, DbError> {
    db.write(|tx| {
        let previous: Option<String> = tx
            .query_row(
                "SELECT manifest_hash FROM pool_version WHERE id = ?1",
                params![pool.version],
                |r| r.get(0),
            )
            .ok();

        match previous.as_deref() {
            Some(hash) if hash == pool.manifest_hash => return Ok(Recorded::Unchanged),
            _ => {}
        }

        tx.execute(
            "INSERT INTO pool_version (id, manifest_hash, image_count, created_at)
                  VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (id) DO UPDATE
                    SET manifest_hash = excluded.manifest_hash,
                        image_count   = excluded.image_count,
                        created_at    = excluded.created_at",
            params![pool.version, pool.manifest_hash, pool.images.len(), now],
        )?;

        // Replaced wholesale rather than merged: the index *is* the identity here — it is what the
        // derivation draws against — so a re-cut that shifted every image by one would otherwise
        // leave a table that is half of one pool and half of another.
        tx.execute(
            "DELETE FROM pool_image WHERE pool_version = ?1",
            params![pool.version],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO pool_image (pool_version, idx, image_id, category)
                      VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (idx, image) in pool.images.iter().enumerate() {
                insert.execute(params![pool.version, idx, image.id, image.category])?;
            }
        }

        Ok(match previous {
            Some(was) => Recorded::Recut { was },
            None => Recorded::First,
        })
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Recorded {
    /// This version had not been recorded before.
    First,
    /// Already recorded, pointing at the same manifest.
    Unchanged,
    /// Already recorded, pointing at a **different** manifest (D34). Carries the hash it used to
    /// point at, because that is the string an operator needs in order to find the old file.
    Recut { was: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::ImageEntry;

    fn manifest(version: u32, images: usize) -> Manifest {
        let categories: Vec<String> = (0..2).map(|c| format!("cat{c}")).collect();
        let images: Vec<ImageEntry> = (0..images)
            .map(|i| ImageEntry {
                id: format!("img_{i:03}"),
                category: format!("cat{}", i % 2),
            })
            .collect();
        Manifest {
            version,
            manifest_hash: Manifest::compute_hash(&categories, &images),
            categories,
            images,
        }
    }

    fn images_of(db: &Db, version: u32) -> Vec<(i64, String)> {
        let reader = db.reader().unwrap();
        let mut stmt = reader
            .prepare("SELECT idx, image_id FROM pool_image WHERE pool_version = ?1 ORDER BY idx")
            .unwrap();
        stmt.query_map(params![version], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn the_served_pool_and_its_images_are_written_once() {
        let db = Db::open_in_memory().unwrap();
        let pool = manifest(1, 4);

        assert_eq!(
            served(&db, &pool, "2026-07-28T09:00:00Z").unwrap(),
            Recorded::First
        );
        let listed = images_of(&db, 1);
        assert_eq!(listed.len(), 4);
        // The index is the order the derivation draws against, so it is what gets recorded.
        assert_eq!(listed[0], (0, "img_000".to_string()));
        assert_eq!(listed[3], (3, "img_003".to_string()));

        // The second process starting against the same file finds its own row.
        assert_eq!(
            served(&db, &pool, "2026-07-28T09:00:01Z").unwrap(),
            Recorded::Unchanged
        );
        assert_eq!(images_of(&db, 1).len(), 4, "and writes nothing twice");
    }

    /// D34 allows the re-cut; this makes it loud. The image list is replaced wholesale, because a
    /// table half of one pool and half of another describes a manifest that never existed.
    #[test]
    fn a_version_that_points_somewhere_new_is_reported_and_replaced() {
        let db = Db::open_in_memory().unwrap();
        let first = manifest(1, 4);
        served(&db, &first, "2026-07-28T09:00:00Z").unwrap();

        let recut = manifest(1, 2);
        let outcome = served(&db, &recut, "2026-07-28T10:00:00Z").unwrap();
        assert_eq!(
            outcome,
            Recorded::Recut {
                was: first.manifest_hash.clone()
            }
        );
        assert_eq!(images_of(&db, 1).len(), 2, "no leftovers from the old cut");

        let (hash, count): (String, i64) = {
            let reader = db.reader().unwrap();
            reader
                .query_row(
                    "SELECT manifest_hash, image_count FROM pool_version WHERE id = 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        assert_eq!(hash, recut.manifest_hash);
        assert_eq!(count, 2);
    }
}
