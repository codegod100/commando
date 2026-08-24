/// Conservative blocklist for accidental destruction. This is not a sandbox.
const BLOCKED: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -fr /",
    "rm -fr /*",
    "mkfs",
    "mkfs.",
    ":(){",
    ":() {",
    "fork bomb",
    "dd if=/dev/zero of=/dev/",
    "dd if=/dev/random of=/dev/",
    "> /dev/sd",
    "of=/dev/sd",
    "of=/dev/nvme",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init 0",
    "init 6",
    "systemctl poweroff",
    "systemctl reboot",
    "systemctl halt",
    "wipefs",
    "chmod -r 777 /",
    "chmod -r 777 /*",
    "chown -r ",
];

pub fn blocked_command(command: &str) -> Option<&'static str> {
    let normalized = collapse_ws(&command.to_ascii_lowercase());
    BLOCKED
        .iter()
        .copied()
        .find(|needle| normalized.contains(&collapse_ws(needle)))
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_root_wipe() {
        assert!(blocked_command("sudo rm -rf /").is_some());
        assert!(blocked_command("rm  -rf   /*").is_some());
    }

    #[test]
    fn allows_normal_work() {
        assert!(blocked_command("rm -rf ./build").is_none());
        assert!(blocked_command("ls -la").is_none());
        assert!(blocked_command("mkdir -p invoices").is_none());
    }
}
