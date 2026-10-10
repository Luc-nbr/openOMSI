use super::*;

/// A quality preset keeps the graphics mode and what is not graphics; Vanilla's leave
/// out what OMSI 2 does not draw, Enhanced+'s keep its traced shadows, occlusion and
/// reflections; High of Vanilla+ is the defaults.
#[test]
fn quality_presets_keep_the_mode_and_the_rest() {
    for mode in ["vanilla", "vanilla_plus", "enhanced", "enhanced_plus"] {
        for (_, preset) in graphics_presets_for(mode) {
            let mut v = settings_from_text(Some(&format!("graphics={mode}\ngraphics_api=gl\nlanguage=PTB\nai_unsched_factor=25\n")));
            apply_graphics_profile(&preset, &mut v);
            let saved = settings_from_text(Some(&settings_to_text(&v, None)));
            assert_eq!(saved["graphics"], mode);
            assert_eq!(saved["graphics_api"], "gl");
            assert_eq!(saved["language"], "PTB");
            assert_eq!(saved["ai_unsched_factor"], 25);
            for key in ["ssao", "shadows", "detail_textures", "reflections", "mirror_refresh", "texture_memory"] {
                assert_eq!(saved[key], preset[key], "{mode}: {key}");
            }
        }
    }
    for (_, p) in graphics_presets_for("OMSI 2") {
        assert!(p["ssao"] == false && p["shadows"] == false && p["detail_textures"] == false);
    }
    for (_, p) in graphics_presets_for("Enhanced+") {
        assert!(p["ssao"] == true && p["shadows"] == true && p["reflections"] == true);
    }
    let defaults = settings_from_text(None);
    let high = &graphics_presets_for("vanilla_plus")[2].1;
    for (k, x) in high.as_object().unwrap() {
        assert_eq!(defaults[k.as_str()].to_string().trim_matches('"'), x.to_string().trim_matches('"'), "{k}");
    }
}

/// The enhanced clouds' quality is saved by the launcher and kept in graphics profiles.
#[test]
fn cloud_quality_is_saved_and_kept_in_graphics_profiles() {
    assert_eq!(settings_from_text(None)["cloud_quality"], "high");
    for q in ["high", "low"] {
        let v = settings_from_text(Some(&format!("graphics=enhanced\ncloud_quality={q}\n")));
        assert_eq!(settings_from_text(Some(&settings_to_text(&v, None)))["cloud_quality"], q);
    }
    assert_eq!(settings_from_text(Some("cloud_quality=auto"))["cloud_quality"], "high");
    assert!(GRAPHICS_PROFILE_KEYS.contains(&"cloud_quality"));
}

#[test]
fn the_day_before_crosses_months_and_years() {
    assert_eq!(day_before(20261007), 20261006);
    assert_eq!(day_before(20261001), 20260930);
    assert_eq!(day_before(20240301), 20240229);
    assert_eq!(day_before(20260101), 20251231);
}

#[test]
fn deleting_one_driver_keeps_other_and_unowned_runs() {
    let dir = std::env::temp_dir().join(format!("omsi_profile_cleanup_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |file: &str, driver: &str| {
        let session = Session {
            driver: driver.into(),
            ..Default::default()
        };
        std::fs::write(dir.join(file), serde_json::to_vec(&session).unwrap()).unwrap();
    };
    write("mine.json", "Isaac");
    write("mine-case.json", "ISAAC");
    write("other.json", "Isaac S.");
    std::fs::write(dir.join("unknown.json"), "{broken").unwrap();
    std::fs::create_dir_all(dir.join("folder.json")).unwrap();
    delete_profile_sessions(&dir, "isaac").unwrap();
    assert!(!dir.join("mine.json").exists());
    assert!(!dir.join("mine-case.json").exists());
    assert!(dir.join("other.json").exists());
    assert!(dir.join("unknown.json").exists());
    assert!(dir.join("folder.json").is_dir());
    // Repeating deletion, and a profile with no history directory, are harmless.
    delete_profile_sessions(&dir, "Isaac").unwrap();
    delete_profile_sessions(&dir.join("absent"), "Isaac").unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_delete_request_needs_a_plain_profile_name() {
    for name in ["", " ", "../other", "a\\b", "C:other"] {
        assert!(delete_profile(name).is_err());
    }
}
