pub mod install;
pub mod reinstall;
pub mod status;
pub mod theme;
pub mod uninstall;
pub mod update;

pub const MARKER: &str = "//# spotiflow";
pub const THEME: &str = include_str!("../../assets/css/theme.css");
