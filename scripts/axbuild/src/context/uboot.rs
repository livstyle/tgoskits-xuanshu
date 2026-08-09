use ostool::run::uboot::UbootConfig;

/// Ostool's `LocalBackend` reads `[net]` from `UbootConfig.local`, but TOML `[net]`
/// tables deserialize into the top-level `UbootConfig.net` field (declared before
/// `#[serde(flatten)] local`). Move shared local-only fields so TFTP boot works.
pub(crate) fn normalize_uboot_config_for_local_backend(config: &mut UbootConfig) {
    if config.local.net.is_none() {
        config.local.net = config.net.take();
    }
    if config.local.board_reset_cmd.is_none() {
        config.local.board_reset_cmd = config.board_reset_cmd.take();
    }
    if config.local.board_power_off_cmd.is_none() {
        config.local.board_power_off_cmd = config.board_power_off_cmd.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_moves_top_level_net_into_local() {
        let mut config: UbootConfig = toml::from_str(
            r#"
serial = "/dev/ttyUSB0"
baud_rate = "1500000"
success_regex = []
fail_regex = []
[net]
interface = "enp3s0"
tftp_dir = "/tmp/ostool-tftp"
"#,
        )
        .unwrap();

        assert!(config.net.is_some());
        assert!(config.local.net.is_none());

        normalize_uboot_config_for_local_backend(&mut config);

        assert!(config.net.is_none());
        let net = config.local.net.as_ref().unwrap();
        assert_eq!(net.interface, "enp3s0");
        assert_eq!(net.tftp_dir.as_deref(), Some("/tmp/ostool-tftp"));
    }
}
