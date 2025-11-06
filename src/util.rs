use std::process::Command;
use crate::VERSION;

use anyhow::Result;

pub fn run_command_in_container(name: &str, uid: u32, auth_key: &str, command: &str) -> Result<String> {
    let args = format!("run --rm --privileged -w /home/user -e HOME=/home/user -e NODE={name} -e TS_AUTHKEY={auth_key} -u {uid} -v ./home:/home/user/ -v ./tailscale-data:/var/lib/tailscale --entrypoint /bin/bash hhanh00/zcash-vote-docker:{VERSION} -c '{command}'");
    let args = shell_words::split(&args)?;
    let output = Command::new("docker")
        .args(&args)
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout)
}
