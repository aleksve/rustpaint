//! Font discovery for the text tool.
//!
//! `fontdb` scans the fonts installed on the machine (registry on Windows,
//! `/System/Library/Fonts` + `~/Library/Fonts` on macOS, the usual font
//! directories on Linux) on a background thread so window startup never blocks
//! on machines with thousands of installed faces. `ab_glyph` then rasterises
//! outlines (both TrueType and CFF/OTF) on demand, cached by `(family, bold)`.
//!
//! The two bundled faces are always present immediately, so the text tool works
//! even before discovery finishes or on a machine with no system fonts at all.

use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::{self, Receiver};

use ab_glyph::FontArc;
use fontdb::{Database, Family, Language, Query, Stretch, Style, Weight};

use crate::assets::{FALLBACK_FONT, UI_FONT};

pub(crate) const BUILTIN_INTER: &str = "Inter (bundled)";
pub(crate) const BUILTIN_DEJAVU: &str = "DejaVu Sans (bundled)";

/// A loaded face plus whether the file really is a bold weight (if not, bold
/// text is synthesised by offsetting a second pass).
pub(crate) type Face = (FontArc, bool);

/// Per-family cache slots: `[regular, bold]`. `None` = not yet queried;
/// `Some(None)` = queried and missing; `Some(Some(face))` = loaded.
type FamilySlots = [Option<Option<Face>>; 2];

struct DiscoveredFonts {
    db: Option<Database>,
    system_families: Vec<String>,
}

fn discover_system_fonts() -> DiscoveredFonts {
    let mut db = Database::new();
    db.load_system_fonts();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for face in db.faces() {
        // Prefer an English family name when the font declares multiple
        // localised names, falling back to the first entry.
        let picked = face
            .families
            .iter()
            .find(|(_, lang)| *lang == Language::English_UnitedStates)
            .or_else(|| face.families.first());
        if let Some((name, _)) = picked {
            let name = name.trim();
            if !name.is_empty() && name != BUILTIN_INTER && name != BUILTIN_DEJAVU {
                seen.insert(name.to_string());
            }
        }
    }

    let system_families: Vec<String> = seen.into_iter().collect();
    let db = if system_families.is_empty() {
        None
    } else {
        Some(db)
    };
    DiscoveredFonts {
        db,
        system_families,
    }
}

pub(crate) struct FontCatalog {
    db: Option<Database>,
    families: Vec<String>,
    pending: Option<Receiver<DiscoveredFonts>>,
    pending_pref: Option<String>,
    /// Keyed by `Box<str>` so cache hits look up via `&str` with zero allocation.
    cache: HashMap<Box<str>, FamilySlots>,
    bundled_inter: Option<FontArc>,
    bundled_dejavu: Option<FontArc>,
}

impl FontCatalog {
    /// Start with the bundled faces immediately and scan system fonts on a
    /// background thread so application launch stays instantaneous.
    pub fn new_async(preferred_family: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("rustpaint-fonts".into())
            .spawn(move || {
                let _ = tx.send(discover_system_fonts());
            })
            .is_ok();

        Self {
            db: None,
            families: vec![BUILTIN_INTER.to_string(), BUILTIN_DEJAVU.to_string()],
            pending: spawned.then_some(rx),
            pending_pref: preferred_family,
            cache: HashMap::new(),
            bundled_inter: FontArc::try_from_slice(UI_FONT).ok(),
            bundled_dejavu: FontArc::try_from_slice(FALLBACK_FONT).ok(),
        }
    }

    /// Lightweight catalogue containing only the embedded faces (fast for tests).
    #[cfg(test)]
    pub fn bundled_only() -> Self {
        Self {
            db: None,
            families: vec![BUILTIN_INTER.to_string(), BUILTIN_DEJAVU.to_string()],
            pending: None,
            pending_pref: None,
            cache: HashMap::new(),
            bundled_inter: FontArc::try_from_slice(UI_FONT).ok(),
            bundled_dejavu: FontArc::try_from_slice(FALLBACK_FONT).ok(),
        }
    }

    /// Absorb completed background discovery results without blocking.
    /// Returns the user's preferred saved family if it just became available.
    pub fn poll_ready(&mut self) -> Option<String> {
        let rx = self.pending.as_ref()?;
        match rx.try_recv() {
            Ok(found) => {
                self.pending = None;
                self.apply_discovered(found)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.pending_pref = None;
                None
            }
        }
    }

    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    fn apply_discovered(&mut self, found: DiscoveredFonts) -> Option<String> {
        self.db = found.db;
        self.families.extend(found.system_families);
        let pref = self.pending_pref.take()?;
        self.families.iter().any(|f| f == &pref).then_some(pref)
    }

    /// All selectable family names: bundled faces first, then system families sorted.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// Load `family` at regular or bold weight. The second tuple element is
    /// `true` when the face is a genuine bold design.
    pub fn face(&mut self, family: &str, bold: bool) -> Option<Face> {
        match family {
            BUILTIN_INTER => return self.bundled_inter.clone().map(|f| (f, false)),
            BUILTIN_DEJAVU => return self.bundled_dejavu.clone().map(|f| (f, false)),
            _ => {}
        }
        let slot_idx = usize::from(bold);
        if let Some(slots) = self.cache.get(family)
            && let Some(cached) = &slots[slot_idx]
        {
            return cached.clone();
        }
        let loaded = self.load_system_face(family, bold);
        let slots = self
            .cache
            .entry(family.into())
            .or_insert_with(|| [None, None]);
        slots[slot_idx] = Some(loaded.clone());
        loaded
    }

    fn load_system_face(&self, family: &str, bold: bool) -> Option<Face> {
        let db = self.db.as_ref()?;
        let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
        let query = Query {
            families: &[Family::Name(family)],
            weight,
            stretch: Stretch::Normal,
            style: Style::Normal,
        };
        let id = db.query(&query)?;
        let true_bold = db.face(id).is_some_and(|f| f.weight.0 >= 600);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_faces_always_load() {
        let mut cat = FontCatalog::bundled_only();
        assert_eq!(cat.families().len(), 2);
        assert!(cat.face(BUILTIN_INTER, false).is_some());
        assert!(cat.face(BUILTIN_INTER, true).is_some());
        assert!(cat.face(BUILTIN_DEJAVU, false).is_some());
        // Unknown family must not panic, just return None (and cache the miss).
        assert!(cat.face("No Such Family 42", false).is_none());
        assert!(cat.face("No Such Family 42", false).is_none());
    }
}
