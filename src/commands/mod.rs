pub mod install;
pub mod reinstall;
pub mod status;
pub mod theme;
pub mod uninstall;
pub mod update;

/// Appended to the patched UI bundle so later runs can tell it apart from a stock client.
pub const MARKER: &str = "//# spotiflow";
pub const THEME: &str = include_str!("../../assets/css/theme.css");
