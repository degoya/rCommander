//! Reading connections that other tools already know, for the connection manager's import, and
//! Total Commander's hotlist, tabs, FTP connections (RC-050-09), colours and font (RC-050-14); Remote Desktop Manager's
//! exports (RC-050-11).

pub(crate) mod bounded_read;
pub mod rdm;
pub mod rdm_report;
pub mod ssh_config;
pub mod ssh_config_apply;
pub mod ssh_config_path;
pub mod tc_ini;
pub mod tc_password;
pub mod total_commander;
pub(crate) mod total_commander_apply;
pub(crate) mod total_commander_load;
pub mod total_commander_look;
pub(crate) mod total_commander_look_apply;
pub mod total_commander_path;
pub mod total_commander_report;
