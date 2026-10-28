use std::collections::{BTreeMap, BTreeSet};

use crate::services::error::ServiceError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FontFamily {
    family: String,
    styles: Vec<String>,
}

impl FontFamily {
    pub(crate) fn family(&self) -> &str {
        &self.family
    }

    pub(crate) fn styles(&self) -> &[String] {
        &self.styles
    }
}

pub(crate) struct FontService;

impl FontService {
    pub(crate) fn list_system_fonts() -> Result<Vec<FontFamily>, ServiceError> {
        let mut database = fontdb::Database::new();
        database.load_system_fonts();
        let entries: Vec<_> = database
            .faces()
            .filter_map(|face| {
                let family = face.families.first().map(|(name, _)| name.as_str());
                font_entry(family, style_name(face.style))
            })
            .collect();
        font_families_from_entries(entries)
    }
}

fn style_name(style: fontdb::Style) -> &'static str {
    match style {
        fontdb::Style::Normal => "Regular",
        fontdb::Style::Italic => "Italic",
        fontdb::Style::Oblique => "Oblique",
    }
}

fn font_entry(family: Option<&str>, style: &str) -> Option<(String, String)> {
    let family = family.filter(|family| !family.is_empty())?;
    if style.is_empty() {
        return None;
    }
    Some((family.to_owned(), style.to_owned()))
}

fn group_font_faces(entries: impl IntoIterator<Item = (String, String)>) -> Vec<FontFamily> {
    let mut by_family = BTreeMap::<String, BTreeSet<String>>::new();
    for (family, style) in entries {
        if family.is_empty() || style.is_empty() {
            continue;
        }
        by_family.entry(family).or_default().insert(style);
    }
    by_family
        .into_iter()
        .map(|(family, styles)| FontFamily {
            family,
            styles: styles.into_iter().collect(),
        })
        .collect()
}

fn font_families_from_entries(
    entries: impl IntoIterator<Item = (String, String)>,
) -> Result<Vec<FontFamily>, ServiceError> {
    let families = group_font_faces(entries);
    if families.is_empty() {
        Err(ServiceError::Internal("No system fonts found".into()))
    } else {
        Ok(families)
    }
}

#[cfg(test)]
mod tests {
    use super::{font_entry, font_families_from_entries, group_font_faces, FontService};

    #[test]
    fn groups_families_and_styles_in_sorted_deduplicated_order() {
        let fonts = group_font_faces(vec![
            ("Zapf Dingbats".into(), "Regular".into()),
            ("Arial".into(), "Bold".into()),
            ("Arial".into(), "Regular".into()),
            ("Arial".into(), "Regular".into()),
        ]);

        assert_eq!(
            fonts.iter().map(|font| font.family()).collect::<Vec<_>>(),
            ["Arial", "Zapf Dingbats"]
        );
        assert_eq!(fonts[0].styles(), ["Bold", "Regular"]);
        assert_eq!(fonts[1].styles(), ["Regular"]);
    }

    #[test]
    fn skips_missing_empty_and_malformed_face_metadata() {
        assert_eq!(font_entry(None, "Regular"), None);
        assert_eq!(font_entry(Some(""), "Regular"), None);
        assert_eq!(font_entry(Some("Arial"), ""), None);
        assert_eq!(
            font_entry(Some("Arial"), "Italic"),
            Some(("Arial".into(), "Italic".into()))
        );
    }

    #[test]
    fn empty_grouping_result_returns_typed_error() {
        for entries in [Vec::new(), vec![(String::new(), "Regular".into())]] {
            let error = font_families_from_entries(entries).unwrap_err();
            assert_eq!(error.code(), "E_INTERNAL");
        }
    }

    #[test]
    fn system_enumeration_has_no_machine_specific_count_assumption() {
        match FontService::list_system_fonts() {
            Ok(fonts) => {
                assert!(fonts
                    .windows(2)
                    .all(|pair| pair[0].family() < pair[1].family()));
                assert!(fonts.iter().all(|font| !font.family().is_empty()));
            }
            Err(error) => assert_eq!(error.code(), "E_INTERNAL"),
        }
    }
}
