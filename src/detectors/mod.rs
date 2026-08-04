pub mod cargo;
pub mod composer;
pub mod js;
pub mod justfile;
pub mod mise;
pub mod node_conventions;
pub mod npm;
pub mod nx;
pub mod php_conventions;
pub mod pnpm;
pub mod util;
pub mod yarn;

use crate::detect::DetectorGroup;

/// Return all built-in detectors as exclusive groups.
///
/// Each `DetectorGroup` is an **exclusive group**: the first detector whose
/// `detect()` returns true wins, and the rest of the group is skipped.
/// Single-element groups behave as independent detectors.
///
/// Group ordering encodes mutual-exclusion invariants:
/// - Tier 2: justfile beats mise; nx is independent, overlapping names
///   resolve by TaskRunner priority (just 10 > mise 5 > nx 3)
/// - Tier 3 JS: pnpm beats yarn beats npm (later: bun goes before these).
///   This ordering mirrors the precedence table in [`js`]; keep the two in sync.
pub fn all_detectors() -> Vec<DetectorGroup> {
    vec![
        // Tier 2 task runners: justfile beats mise
        DetectorGroup::new(vec![
            Box::new(justfile::JustfileDetector),
            Box::new(mise::MiseDetector),
        ]),
        DetectorGroup::new(vec![Box::new(nx::NxDetector)]),
        // Tier 3 JS: pnpm beats yarn beats npm
        DetectorGroup::new(vec![
            Box::new(pnpm::PnpmDetector),
            Box::new(yarn::YarnDetector),
            Box::new(npm::NpmDetector),
        ]),
        // Tier 3 PHP
        DetectorGroup::new(vec![Box::new(composer::ComposerDetector)]),
        // Tier 4: each independent
        DetectorGroup::new(vec![Box::new(cargo::CargoDetector)]),
        DetectorGroup::new(vec![Box::new(php_conventions::PestDetector)]),
        DetectorGroup::new(vec![Box::new(php_conventions::PhpunitDetector)]),
        DetectorGroup::new(vec![Box::new(php_conventions::PhpstanDetector)]),
        DetectorGroup::new(vec![Box::new(php_conventions::PhpCsFixerDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::VitestDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::JestDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::EslintDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::PrettierDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::BiomeDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::TscDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::PlaywrightDetector)]),
        DetectorGroup::new(vec![Box::new(node_conventions::CypressDetector)]),
    ]
}
