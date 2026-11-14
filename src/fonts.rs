use crate::Result;
use fontdb::Database;
use std::collections::BTreeMap;

#[tauri::command]
pub async fn list_system_fonts() -> Result<Vec<String>> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        monospace_families(&database)
    })
    .await
    .map_err(|error| format!("Unable to detect system fonts: {error}"))
}

fn monospace_families(database: &Database) -> Vec<String> {
    let mut families = BTreeMap::new();
    for face in database.faces().filter(|face| face.monospaced) {
        // fontdb puts the English family first, when available. Styles and
        // localized aliases belong to the same entry in the font picker.
        if let Some((family, _)) = face.families.first() {
            let family = family.trim();
            if !family.is_empty() {
                families
                    .entry(family.to_lowercase())
                    .or_insert_with(|| family.to_string());
            }
        }
    }
    families.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontdb::{FaceInfo, ID, Language, Source, Stretch, Style, Weight};

    fn face(families: &[&str], monospaced: bool, weight: Weight) -> FaceInfo {
        FaceInfo {
            id: ID::dummy(),
            source: Source::File("unused.ttf".into()),
            index: 0,
            families: families
                .iter()
                .map(|family| (family.to_string(), Language::English_UnitedStates))
                .collect(),
            post_script_name: String::new(),
            style: Style::Normal,
            weight,
            stretch: Stretch::Normal,
            monospaced,
        }
    }

    #[test]
    fn picker_lists_sorted_monospace_families_without_duplicate_styles_or_aliases() {
        let mut database = Database::new();
        database.push_face_info(face(&["Zed Mono"], true, Weight::NORMAL));
        database.push_face_info(face(
            &["JetBrainsMono Nerd Font Mono", "JetBrainsMono NFM"],
            true,
            Weight::NORMAL,
        ));
        database.push_face_info(face(&["Zed Mono"], true, Weight::BOLD));
        database.push_face_info(face(&["zed mono"], true, Weight::NORMAL));
        database.push_face_info(face(&["Adwaita Mono"], true, Weight::NORMAL));
        database.push_face_info(face(&["Proportional Sans"], false, Weight::NORMAL));
        database.push_face_info(face(&[" "], true, Weight::NORMAL));
        database.push_face_info(face(&[], true, Weight::NORMAL));
        assert_eq!(
            monospace_families(&database),
            ["Adwaita Mono", "JetBrainsMono Nerd Font Mono", "Zed Mono"]
        );
    }

    #[test]
    fn an_empty_database_has_no_installed_families() {
        assert!(monospace_families(&Database::new()).is_empty());
    }

    #[test]
    fn system_font_command_scans_the_host_and_returns_a_serializable_list() {
        let families = tauri::async_runtime::block_on(list_system_fonts()).unwrap();
        assert!(families.iter().all(|family| !family.trim().is_empty()));
        assert!(
            families
                .windows(2)
                .all(|pair| pair[0].to_lowercase() < pair[1].to_lowercase())
        );
        let wire = serde_json::to_value(&families).unwrap();
        assert!(wire.is_array());
        println!("Detected {} monospace families: {wire}", families.len());
    }
}
