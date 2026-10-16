//! Omsi-Hub's own translations: the tables of the texts only Omsi-Hub's launcher page shows -
//! the bus company, RosterFlow, the page's parts of the editors - kept out of the game's
//! `crates/omsi-app/locales` (docs/omsihub/ADDON_PLAN.md, phase 2 step 11). The game is built
//! with its own tables only; the launcher page takes these in with them
//! (`omsi-launcher-api`'s `omsihub::with_ours`).
//!
//! The files are the game's format (`_version: 2`, a text and its translation per language),
//! so a text can move between the two folders as it is.

/// Every table of `locales/`: its file name and its text.
pub const TABLES: &[(&str, &str)] = &[
    ("app-theme.yml", include_str!("../locales/app-theme.yml")),
    ("bedrijfsdienst.yml", include_str!("../locales/bedrijfsdienst.yml")),
    ("bugs-company.yml", include_str!("../locales/bugs-company.yml")),
    ("casual.yml", include_str!("../locales/casual.yml")),
    ("cloud-saves.yml", include_str!("../locales/cloud-saves.yml")),
    ("company-agencies.yml", include_str!("../locales/company-agencies.yml")),
    ("company-agenda.yml", include_str!("../locales/company-agenda.yml")),
    ("company-breakdowns.yml", include_str!("../locales/company-breakdowns.yml")),
    ("company-chooser.yml", include_str!("../locales/company-chooser.yml")),
    ("company-complaints.yml", include_str!("../locales/company-complaints.yml")),
    ("company-contracts.yml", include_str!("../locales/company-contracts.yml")),
    ("company-currency.yml", include_str!("../locales/company-currency.yml")),
    ("company-drive.yml", include_str!("../locales/company-drive.yml")),
    ("company-easy.yml", include_str!("../locales/company-easy.yml")),
    ("company-holidays.yml", include_str!("../locales/company-holidays.yml")),
    ("company-home.yml", include_str!("../locales/company-home.yml")),
    ("company-lateness.yml", include_str!("../locales/company-lateness.yml")),
    ("company-livemap.yml", include_str!("../locales/company-livemap.yml")),
    ("company-multiplayer.yml", include_str!("../locales/company-multiplayer.yml")),
    ("company-names.yml", include_str!("../locales/company-names.yml")),
    ("company-ops.yml", include_str!("../locales/company-ops.yml")),
    ("company-pages.yml", include_str!("../locales/company-pages.yml")),
    ("company-planning.yml", include_str!("../locales/company-planning.yml")),
    ("company-realtime.yml", include_str!("../locales/company-realtime.yml")),
    ("company-reputation.yml", include_str!("../locales/company-reputation.yml")),
    ("company-research.yml", include_str!("../locales/company-research.yml")),
    ("company-staff.yml", include_str!("../locales/company-staff.yml")),
    ("company-tender-score.yml", include_str!("../locales/company-tender-score.yml")),
    ("company-together-gate.yml", include_str!("../locales/company-together-gate.yml")),
    ("company-together-leave.yml", include_str!("../locales/company-together-leave.yml")),
    ("company-together.yml", include_str!("../locales/company-together.yml")),
    ("dealer-delivery.yml", include_str!("../locales/dealer-delivery.yml")),
    ("dealer.yml", include_str!("../locales/dealer.yml")),
    ("easy-polish.yml", include_str!("../locales/easy-polish.yml")),
    ("fix_core_company_economy.yml", include_str!("../locales/fix_core_company_economy.yml")),
    ("lijnen.yml", include_str!("../locales/lijnen.yml")),
    ("lijnkleur.yml", include_str!("../locales/lijnkleur.yml")),
    ("logo.yml", include_str!("../locales/logo.yml")),
    ("openomsi-0230.yml", include_str!("../locales/openomsi-0230.yml")),
    ("planning-studio.yml", include_str!("../locales/planning-studio.yml")),
    ("ritkaart.yml", include_str!("../locales/ritkaart.yml")),
    ("rosterflow-breaks.yml", include_str!("../locales/rosterflow-breaks.yml")),
    ("rosterflow-days.yml", include_str!("../locales/rosterflow-days.yml")),
    ("rosterflow-gantt.yml", include_str!("../locales/rosterflow-gantt.yml")),
    ("rosterflow-optimise.yml", include_str!("../locales/rosterflow-optimise.yml")),
    ("rosterflow-relief.yml", include_str!("../locales/rosterflow-relief.yml")),
    ("rosterflow-roster-tab.yml", include_str!("../locales/rosterflow-roster-tab.yml")),
    ("rosterflow-routes.yml", include_str!("../locales/rosterflow-routes.yml")),
    ("rosterflow-rules.yml", include_str!("../locales/rosterflow-rules.yml")),
    ("rosterflow-spares.yml", include_str!("../locales/rosterflow-spares.yml")),
    ("rosterflow-timeplan.yml", include_str!("../locales/rosterflow-timeplan.yml")),
    ("rosterflow-timetable-copy.yml", include_str!("../locales/rosterflow-timetable-copy.yml")),
    ("rosterflow-together.yml", include_str!("../locales/rosterflow-together.yml")),
    ("rosterflow-tutorial.yml", include_str!("../locales/rosterflow-tutorial.yml")),
    ("rosterflow-unassign.yml", include_str!("../locales/rosterflow-unassign.yml")),
    ("rosterflow.yml", include_str!("../locales/rosterflow.yml")),
    ("sandbox.yml", include_str!("../locales/sandbox.yml")),
    ("sessie.yml", include_str!("../locales/sessie.yml")),
    ("slepen.yml", include_str!("../locales/slepen.yml")),
    ("tauri-company-more.yml", include_str!("../locales/tauri-company-more.yml")),
    ("tauri-company.yml", include_str!("../locales/tauri-company.yml")),
    ("tauri-omsihub.yml", include_str!("../locales/tauri-omsihub.yml")),
    ("tauri-rosterflow.yml", include_str!("../locales/tauri-rosterflow.yml")),
    ("tijd.yml", include_str!("../locales/tijd.yml")),
    ("vehicle-age.yml", include_str!("../locales/vehicle-age.yml")),
    ("verwijderen.yml", include_str!("../locales/verwijderen.yml")),
];

#[cfg(test)]
mod tests {
    use super::TABLES;

    /// Every file of the folder is in `TABLES` (a new one is taken in too), each a table.
    #[test]
    fn every_table_of_the_folder_is_listed() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let mut files: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".yml")).collect();
        files.sort();
        let listed: Vec<String> = TABLES.iter().map(|t| t.0.to_string()).collect();
        assert_eq!(files, listed);
        for (name, text) in TABLES {
            assert!(text.replace("\r\n", "\n").starts_with("_version: 2"), "{name}");
        }
    }
}
