use std::process::Command;

pub fn command_exists(program: &str) -> bool {
    Command::new("where")
        .arg(program)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn read_command_output(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|output| !output.is_empty())
}
