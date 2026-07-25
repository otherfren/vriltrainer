//! The operator's command line. See `docs/curation-guide.md` for the workflow it serves.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use poolctl::annotate::{Catalogue, Record, is_free_licence, normalise_category};
use poolctl::spec::PoolSpec;
use poolctl::{check, manifest, normalise};
// One spelling of "now" across the project. The catalogue and the manifest are read next to log
// entries when a trial is being checked by hand, and two timestamp formats would be one more thing
// to reconcile while doing it.
use server::db::now_rfc3339 as now;

#[derive(Parser)]
#[command(
    name = "poolctl",
    about = "Curate, normalise and publish the vriltrainer image pool"
)]
struct Cli {
    /// Working directory: `catalogue.json` plus the normalised images. Not itself published.
    #[arg(long, default_value = "pool", global = true)]
    pool: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Take every image named by the spec files into the catalogue.
    ///
    /// Idempotent: identity is the hash of the normalised bytes, so re-running after adding
    /// entries processes the new ones and leaves the rest alone.
    Import {
        /// The images and their two names. Paths inside it are relative to it.
        #[arg(long, default_value = "pool/images.toml")]
        images: PathBuf,
        /// The categories a pool image may use, in both languages.
        #[arg(long, default_value = "pool/categories.toml")]
        categories: PathBuf,
    },
    /// Normalise an image, record where it came from, and take it into the catalogue.
    Add {
        file: PathBuf,
        #[arg(long)]
        category: String,
        /// The German name shown for this image.
        #[arg(long)]
        de: String,
        /// The English name shown for this image.
        #[arg(long)]
        en: String,
        /// The page the image was found on, not the direct file link — the licence is stated on
        /// the page.
        #[arg(long)]
        source: String,
        /// CC0 or public domain only. Anything demanding a visible credit is refused here.
        #[arg(long)]
        licence: String,
        #[arg(long)]
        attribution: Option<String>,
    },
    /// Correct provenance on an image already in the catalogue.
    Annotate {
        id: String,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        licence: Option<String>,
        #[arg(long)]
        attribution: Option<String>,
    },
    /// Report what would stop a version being cut, and what is merely thin.
    Check,
    /// Write the manifest for a pool version. Immutable once published.
    Build {
        #[arg(long)]
        version: u32,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let catalogue_path = cli.pool.join("catalogue.json");
    // `images/` belongs to the operator — that is where the sources live, in whatever
    // folders they chose. The normalised output is a build artefact and keeps out of the way.
    let images_dir = cli.pool.join("normalised");

    match run(&cli.command, &catalogue_path, &images_dir) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("poolctl: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: &Command, catalogue_path: &Path, images_dir: &Path) -> Result<ExitCode, String> {
    let mut catalogue = Catalogue::load(catalogue_path)?;

    match command {
        Command::Import { images, categories } => {
            let spec = PoolSpec::load(categories, images)?;
            std::fs::create_dir_all(images_dir)
                .map_err(|e| format!("{}: {e}", images_dir.display()))?;

            let (mut taken, mut known, mut updated) = (0usize, 0usize, 0usize);
            for entry in &spec.images {
                if !is_free_licence(&entry.licence) {
                    return Err(format!(
                        "{}: licence {} is not CC0 or public domain. Its attribution would have to \
                         be shown beside the image, which marks one of the eight",
                        entry.path.display(),
                        entry.licence
                    ));
                }
                let bytes = std::fs::read(&entry.path)
                    .map_err(|e| format!("{}: {e}", entry.path.display()))?;
                let image = normalise::normalise(&bytes)
                    .map_err(|e| format!("{}: {e}", entry.path.display()))?;

                // An image already held is not an error to stop on: the spec is a list the
                // operator keeps extending, and re-running it is the normal way to add to a pool.
                //
                // Its metadata is reconciled rather than skipped, because the spec file is the
                // source of truth. Skipping outright would mean that correcting a category in
                // images.toml — the whole reason to keep the pool in a file — silently did
                // nothing, and the file would then describe a catalogue that disagreed with it.
                if let Some(held) = catalogue.get_mut(&image.id) {
                    let was = (
                        held.category.clone(),
                        held.label.clone(),
                        held.source.clone(),
                        held.licence.clone(),
                        held.attribution.clone(),
                    );
                    held.category = entry.category.clone();
                    held.label = Some(entry.label.clone());
                    held.source = entry.source.clone();
                    held.licence = entry.licence.clone();
                    held.attribution = entry.attribution.clone();
                    let now = (
                        held.category.clone(),
                        held.label.clone(),
                        held.source.clone(),
                        held.licence.clone(),
                        held.attribution.clone(),
                    );
                    if was == now {
                        known += 1;
                    } else {
                        println!("{}  {}  updated", image.id, entry.category);
                        updated += 1;
                    }
                    continue;
                }

                catalogue.add(Record {
                    id: image.id.clone(),
                    category: entry.category.clone(),
                    label: Some(entry.label.clone()),
                    source: entry.source.clone(),
                    licence: entry.licence.clone(),
                    attribution: entry.attribution.clone(),
                    added: now(),
                    original: entry
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned()),
                })?;

                let out = images_dir.join(format!("{}.png", image.id));
                std::fs::write(&out, &image.png).map_err(|e| format!("{}: {e}", out.display()))?;
                println!("{}  {}  {}", image.id, entry.category, entry.path.display());
                taken += 1;
            }
            catalogue.save(catalogue_path)?;
            println!(
                "{taken} taken, {updated} updated, {known} unchanged, {} categories declared",
                spec.categories.len()
            );
            Ok(ExitCode::SUCCESS)
        }

        Command::Add {
            file,
            category,
            de,
            en,
            source,
            licence,
            attribution,
        } => {
            if !is_free_licence(licence) {
                return Err(format!(
                    "licence {licence} is not CC0 or public domain. Its attribution would have to \
                     be shown beside the image, which marks one of the eight"
                ));
            }
            let category = normalise_category(category);
            if category.is_empty() {
                return Err("a category is required".into());
            }

            let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let image =
                normalise::normalise(&bytes).map_err(|e| format!("{}: {e}", file.display()))?;

            catalogue.add(Record {
                id: image.id.clone(),
                category: category.clone(),
                label: Some(poolctl::spec::Label {
                    de: de.trim().into(),
                    en: en.trim().into(),
                }),
                source: source.clone(),
                licence: licence.clone(),
                attribution: attribution.clone(),
                added: now(),
                original: file.file_name().map(|n| n.to_string_lossy().into_owned()),
            })?;

            std::fs::create_dir_all(images_dir)
                .map_err(|e| format!("{}: {e}", images_dir.display()))?;
            let out = images_dir.join(format!("{}.png", image.id));
            std::fs::write(&out, &image.png).map_err(|e| format!("{}: {e}", out.display()))?;
            catalogue.save(catalogue_path)?;

            // The manifest entry, echoed back: the curator added one image and gets to see exactly
            // what the pool now holds because of it.
            println!("{}  {}  {}", image.id, category, out.display());
            Ok(ExitCode::SUCCESS)
        }

        Command::Annotate {
            id,
            category,
            source,
            licence,
            attribution,
        } => {
            if let Some(l) = licence
                && !is_free_licence(l)
            {
                return Err(format!("licence {l} is not CC0 or public domain"));
            }
            let record = catalogue
                .get_mut(id)
                .ok_or_else(|| format!("{id} is not in the catalogue"))?;
            if let Some(c) = category {
                record.category = normalise_category(c);
            }
            if let Some(s) = source {
                record.source = s.clone();
            }
            if let Some(l) = licence {
                record.licence = l.clone();
            }
            if let Some(a) = attribution {
                record.attribution = Some(a.clone());
            }
            let echo = format!(
                "{}  {}  {}  {}",
                record.id, record.category, record.licence, record.source
            );
            catalogue.save(catalogue_path)?;
            println!("{echo}");
            Ok(ExitCode::SUCCESS)
        }

        Command::Check => {
            let report = check::check(&catalogue, images_dir);
            for (name, n) in &report.per_category {
                println!("{n:5}  {name}");
            }
            println!(
                "{:5}  images in {} categories",
                report.total,
                report.per_category.len()
            );
            for w in &report.warnings {
                println!("warning: {w}");
            }
            for e in &report.errors {
                eprintln!("error: {e}");
            }
            Ok(if report.ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }

        Command::Build { version, out } => {
            // A version is immutable once published, so what `check` reports as an error blocks the
            // build rather than merely printing.
            let report = check::check(&catalogue, images_dir);
            if !report.ok() {
                for e in &report.errors {
                    eprintln!("error: {e}");
                }
                return Err("pool does not pass check; nothing was written".into());
            }

            let published = manifest::build(&catalogue.images, *version, &now())?;
            let path = out
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("shared/pool/v{version}.json")));
            if path.exists() {
                return Err(format!(
                    "{} already exists. A published version is never edited — trials recorded \
                     under it must stay verifiable, so cut a new version instead",
                    path.display()
                ));
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&path, manifest::to_json(&published)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;

            println!("{}", path.display());
            println!(
                "{} images, {} categories, {}",
                published.count,
                published.categories.len(),
                published.manifest_hash
            );
            Ok(ExitCode::SUCCESS)
        }
    }
}
