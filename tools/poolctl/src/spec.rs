//! The pool as a file the operator edits, rather than as a hundred and sixty command lines.
//!
//! Two TOML files, both hand-written and both read by `poolctl import`:
//!
//! - `categories.toml` — every category a pool image may use, in both languages.
//! - `images.toml` — the images, their categories and their two names.
//!
//! TOML rather than JSON because these are the only files in the project a human types into at
//! length: it takes comments, it survives a trailing comma, and one changed line diffs as one
//! changed line.
//!
//! **Paths are relative to the file that names them**, so the folder layout under `base` is the
//! operator's to arrange — by subject, by source, by the afternoon they were collected, it does not
//! matter. Nothing downstream sees it: identity is the hash of the normalised bytes, and the
//! category is whatever the entry says it is, not what folder the file sits in. Coupling category
//! to directory would have made reorganising the folders a silent re-categorisation of the pool.
//!
//! **The category allowlist is the point of the first file.** Categories are compared by string
//! equality all the way into the derivation, so `landschaft` and `landschaften` are two half-empty
//! buckets rather than one full one. Without the allowlist a typo creates a category silently, and
//! surfaces only as a wrong total in `poolctl check` long after it is in the catalogue.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::annotate::normalise_category;

/// A pair of display names. Neither may be blank: a pool that ships one language and not the other
/// leaves the second domain rendering an identifier at the visitor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub de: String,
    pub en: String,
}

/* ---------- categories.toml ------------------------------------------------ */

#[derive(Debug, Deserialize)]
pub struct CategoryFile {
    #[serde(default, rename = "category")]
    pub categories: Vec<CategorySpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategorySpec {
    /// The identifier the manifest and the derivation use. Lower case, no spaces.
    pub id: String,
    pub de: String,
    pub en: String,
}

/* ---------- images.toml ---------------------------------------------------- */

#[derive(Debug, Default, Deserialize)]
pub struct ImageFile {
    /// Prefixed to every path in this file. Relative to the file itself.
    #[serde(default)]
    pub base: Option<String>,
    /// Fields every entry inherits unless it says otherwise.
    #[serde(default)]
    pub defaults: Inherited,
    /// Images written straight into the file.
    #[serde(default, rename = "image")]
    pub images: Vec<Entry>,
    /// Images grouped under a directory, sharing whatever the group sets.
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
}

/// Everything an entry can inherit. `file`, `de` and `en` are never inheritable — they are what
/// makes an entry an entry.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Inherited {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub licence: Option<String>,
    #[serde(default)]
    pub attribution: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    /// Appended to `base` for every image in the group.
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(flatten)]
    pub shared: Inherited,
    #[serde(default, rename = "image")]
    pub images: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// Relative to `base` plus the group's `dir`. May itself contain subdirectories.
    pub file: String,
    pub de: String,
    pub en: String,
    #[serde(flatten)]
    pub own: Inherited,
}

/* ---------- resolved ------------------------------------------------------- */

/// One image, with inheritance applied and every required field present.
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    pub path: PathBuf,
    pub category: String,
    pub label: Label,
    pub source: String,
    pub licence: String,
    pub attribution: Option<String>,
}

#[derive(Debug)]
pub struct PoolSpec {
    pub categories: BTreeMap<String, Label>,
    pub images: Vec<ResolvedImage>,
}

impl PoolSpec {
    pub fn load(categories_path: &Path, images_path: &Path) -> Result<Self, String> {
        let categories = load_categories(categories_path)?;
        let file: ImageFile = read_toml(images_path)?;

        let root = images_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let base = match &file.base {
            Some(b) => root.join(b),
            None => root,
        };

        let mut images = Vec::new();
        let mut seen: BTreeMap<PathBuf, usize> = BTreeMap::new();

        for entry in &file.images {
            images.push(resolve(entry, &base, &file.defaults, &file.defaults)?);
        }
        for group in &file.groups {
            let dir = match &group.dir {
                Some(d) => base.join(d),
                None => base.clone(),
            };
            for entry in &group.images {
                images.push(resolve(entry, &dir, &group.shared, &file.defaults)?);
            }
        }

        for (i, image) in images.iter().enumerate() {
            let what = format!("{}", image.path.display());
            if !categories.contains_key(&image.category) {
                // The failure this file exists to make loud, at the moment it is made.
                return Err(format!(
                    "{what}: category `{}` is not declared in {}",
                    image.category,
                    categories_path.display()
                ));
            }
            // A duplicate path is a copy-paste, and it would otherwise surface much later as the
            // catalogue refusing an id it already holds — true, but not where the mistake is.
            if let Some(first) = seen.insert(image.path.clone(), i) {
                return Err(format!(
                    "{what}: listed twice (entries {} and {})",
                    first + 1,
                    i + 1
                ));
            }
        }

        Ok(PoolSpec { categories, images })
    }
}

fn load_categories(path: &Path) -> Result<BTreeMap<String, Label>, String> {
    let file: CategoryFile = read_toml(path)?;
    let mut out: BTreeMap<String, Label> = BTreeMap::new();
    for c in &file.categories {
        let id = normalise_category(&c.id);
        if id.is_empty() {
            return Err(format!("{}: a category has an empty id", path.display()));
        }
        let label = check_label(&c.de, &c.en, &format!("category {id}"))?;
        // Two entries for one id is a badly resolved merge, and the second would silently win.
        if out.insert(id.clone(), label).is_some() {
            return Err(format!(
                "{}: category {id} is declared twice",
                path.display()
            ));
        }
    }
    Ok(out)
}

fn resolve(
    entry: &Entry,
    dir: &Path,
    group: &Inherited,
    defaults: &Inherited,
) -> Result<ResolvedImage, String> {
    let path = dir.join(&entry.file);
    let what = path.display().to_string();

    // Image beats group beats file-level defaults, which is the order of decreasing specificity
    // and therefore the only order that is not surprising.
    let pick = |own: &Option<String>, grp: &Option<String>, def: &Option<String>| {
        own.clone().or_else(|| grp.clone()).or_else(|| def.clone())
    };

    let category = pick(&entry.own.category, &group.category, &defaults.category)
        .ok_or_else(|| format!("{what}: no category, and none inherited"))?;
    let source = pick(&entry.own.source, &group.source, &defaults.source)
        .ok_or_else(|| format!("{what}: no source, and none inherited"))?;
    let licence = pick(&entry.own.licence, &group.licence, &defaults.licence)
        .ok_or_else(|| format!("{what}: no licence, and none inherited"))?;

    Ok(ResolvedImage {
        category: normalise_category(&category),
        label: check_label(&entry.de, &entry.en, &what)?,
        source,
        licence,
        attribution: pick(
            &entry.own.attribution,
            &group.attribution,
            &defaults.attribution,
        ),
        path,
    })
}

fn check_label(de: &str, en: &str, what: &str) -> Result<Label, String> {
    if de.trim().is_empty() {
        return Err(format!("{what}: `de` is empty"));
    }
    if en.trim().is_empty() {
        return Err(format!("{what}: `en` is empty"));
    }
    Ok(Label {
        de: de.trim().into(),
        en: en.trim().into(),
    })
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATS: &str = r#"
[[category]]
id = "landscape"
de = "Landschaft"
en = "Landscape"

[[category]]
id = "animal"
de = "Tier"
en = "Animal"
"#;

    fn spec(images: &str) -> Result<PoolSpec, String> {
        let dir = tempdir();
        std::fs::write(dir.join("categories.toml"), CATS).unwrap();
        std::fs::write(dir.join("images.toml"), images).unwrap();
        PoolSpec::load(&dir.join("categories.toml"), &dir.join("images.toml"))
    }

    /// The shape the curation guide documents: a base, a group per folder, inherited licence.
    #[test]
    fn folders_are_the_operators_business_and_inheritance_does_the_repetition() {
        let s = spec(
            r#"
base = "bilder"

[defaults]
licence = "CC0"
source = "https://example.invalid/collection"

[[group]]
dir = "urlaub/2019"
category = "landscape"

  [[group.image]]
  file = "strand.jpg"
  de = "Strand"
  en = "Beach"

  [[group.image]]
  file = "naeher/duene.jpg"
  de = "Düne"
  en = "Dune"
  source = "https://example.invalid/dune"

[[image]]
file = "einzeln/kaefer.jpg"
category = "animal"
de = "Marienkäfer"
en = "Ladybird"
"#,
        )
        .unwrap();

        // Loose entries first, then groups in order. The sequence is not significant — the
        // manifest sorts by id — so it is only what the console prints and what an error names.
        assert_eq!(s.images.len(), 3);
        assert!(s.images[0].path.ends_with("bilder/einzeln/kaefer.jpg"));
        assert!(s.images[1].path.ends_with("bilder/urlaub/2019/strand.jpg"));
        // A subdirectory inside a group entry is just more path.
        assert!(
            s.images[2]
                .path
                .ends_with("bilder/urlaub/2019/naeher/duene.jpg")
        );

        assert_eq!(s.images[0].category, "animal");
        assert_eq!(s.images[1].category, "landscape");
        assert_eq!(s.images[1].licence, "CC0", "inherited from defaults");
        assert_eq!(
            s.images[2].source, "https://example.invalid/dune",
            "the image overrides the default"
        );
    }

    /// The whole reason the allowlist exists: one letter, two half-empty buckets.
    #[test]
    fn a_category_that_is_not_declared_is_refused() {
        let err = spec(
            r#"
[[image]]
file = "x.jpg"
category = "landschaften"
de = "Strand"
en = "Beach"
source = "https://example.invalid/x"
licence = "CC0"
"#,
        )
        .unwrap_err();
        assert!(err.contains("landschaften"), "{err}");
    }

    #[test]
    fn case_and_stray_spaces_are_not_a_second_category() {
        assert!(
            spec(
                r#"
[[image]]
file = "x.jpg"
category = "  Landscape "
de = "Strand"
en = "Beach"
source = "https://example.invalid/x"
licence = "CC0"
"#
            )
            .is_ok()
        );
    }

    #[test]
    fn a_missing_translation_is_refused() {
        let err = spec(
            r#"
[[image]]
file = "x.jpg"
category = "landscape"
de = "Strand"
en = "   "
source = "https://example.invalid/x"
licence = "CC0"
"#,
        )
        .unwrap_err();
        assert!(err.contains("`en` is empty"), "{err}");
    }

    #[test]
    fn an_entry_with_nothing_to_inherit_says_what_is_missing() {
        let err = spec(
            r#"
[[image]]
file = "x.jpg"
de = "Strand"
en = "Beach"
source = "https://example.invalid/x"
licence = "CC0"
"#,
        )
        .unwrap_err();
        assert!(err.contains("no category"), "{err}");
    }

    #[test]
    fn the_same_file_listed_twice_is_refused_where_the_mistake_is() {
        let one = r#"
[[image]]
file = "x.jpg"
category = "landscape"
de = "Strand"
en = "Beach"
source = "https://example.invalid/x"
licence = "CC0"
"#;
        let err = spec(&format!("{one}{one}")).unwrap_err();
        assert!(err.contains("listed twice"), "{err}");
    }

    #[test]
    fn a_category_declared_twice_is_refused() {
        let dir = tempdir();
        std::fs::write(dir.join("categories.toml"), format!("{CATS}{CATS}")).unwrap();
        std::fs::write(dir.join("images.toml"), "").unwrap();
        let err =
            PoolSpec::load(&dir.join("categories.toml"), &dir.join("images.toml")).unwrap_err();
        assert!(err.contains("twice"), "{err}");
    }

    fn tempdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "poolspec-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
