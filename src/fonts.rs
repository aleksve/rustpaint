//! Font discovery for the text tool.
//!
//! `fontdb` scans the fonts installed on the machine (registry on Windows,
//! `/System/Library/Fonts` + `~/Library/Fonts` on macOS, the usual font
//! directories on Linux) and hands us the raw bytes, which `ab_glyph` then
//! rasterises. Both crates are pure Rust, so nothing platform specific ends up
//! in the build.
//!
//! The two bundled faces are always present, which means the text tool has a
//! working font even on a machine with no fonts installed at all.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use ab_glyph::FontArc;
use fontdb::{Database, Family, Query, Stretch, Style, Weight};

use crate::assets::{FALLBACK_FONT, UI_FONT};

pub const BUILTIN_INTER: &str = "Inter (bundled)";
pub const BUILTIN_DEJAVU: &str = "DejaVu Sans (bundled)";

/// A loaded face plus whether the file really is a bold weight (if not, bold
/// text is synthesised by double-striking the glyphs).
type Face = (FontArc, bool);

pub struct FontCatalog {
    db: Option<Database>,
    families: Vec<String>,
    cache: RefCell<HashMap<(String, bool), Option<Face>>>,
}

impl FontCatalog {
    pub fn new() -> Self {
        let mut families = vec![BUILTIN_INTER.to_string(), BUILTIN_DEJAVU.to_string()];

        let mut db = Database::new();
        db.load_system_fonts();

        // Collect unique English family names, alphabetically.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for face in db.faces() {
            if let Some((name, _)) = face.families.first() {
                let name = name.trim();
                if !name.is_empty() && name != BUILTIN_INTER && name != BUILTIN_DEJAVU {
                    seen.insert(name.to_string());
                }
            }
        }
        families.extend(seen);
        let usable = if families.len() > 2 { Some(db) } else { None };

        Self {
            db: usable,
            families,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// All selectable family names: the bundled faces first, then everything
    /// installed on the system, sorted.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    pub fn family_count(&self) -> usize {
        self.families.len()
    }

    /// Load `family` at regular or bold weight. The second tuple element is
    /// `true` when the face is a genuine bold design.
    pub fn face(&self, family: &str, bold: bool) -> Option<Face> {
        let key = (family.to_string(), bold);
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let loaded = self.load_face(family, bold);
        self.cache.borrow_mut().insert(key, loaded.clone());
        loaded
    }

    fn load_face(&self, family: &str, bold: bool) -> Option<Face> {
        match family {
            BUILTIN_INTER => Some((FontArc::try_from_slice(UI_FONT).ok()?, false)),
            BUILTIN_DEJAVU => Some((FontArc::try_from_slice(FALLBACK_FONT).ok()?, false)),
            _ => {
                let db = self.db.as_ref()?;
                let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
                let query = Query {
                    families: &[Family::Name(family)],
                    weight,
                    stretch: Stretch::Normal,
                    style: Style::Normal,
                };
                let id = db.query(&query)?;
                let true_bold = db.face(id).map(|f| f.weight.0 >= 600).unwrap_or(false);
                let arc = db
                    .with_face_data(id, |data, index| {
                        ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index)
                            .ok()
                            .map(FontArc::from)
                    })
                    .flatten()?;
                Some((arc, true_bold))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_faces_always_load() {
        let cat = FontCatalog::new();
        assert!(!cat.families().is_empty());
        assert!(cat.face(BUILTIN_INTER, false).is_some());
        assert!(cat.face(BUILTIN_DEJAVU, false).is_some());
        // Unknown family must not panic, just return None.
        assert!(cat.face("No Such Family 42", false).is_none());
    }
}
