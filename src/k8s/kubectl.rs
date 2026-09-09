use std::process::Stdio;
use std::time::Duration;

const KUBECTL: &str = "kubectl";
pub const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(30);
pub const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn capture(args: &[&str], budget: Duration) -> Result<String, String> {
    let output = tokio::time::timeout(
        budget,
        tokio::process::Command::new(KUBECTL)
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        tracing::warn!("kubectl {} timed out after {budget:?}", args.join(" "));
        format!("kubectl timed out after {}s", budget.as_secs())
    })?
    .map_err(|e| {
        tracing::warn!("kubectl {} failed to spawn: {e}", args.join(" "));
        format!("failed to execute kubectl: {e}")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            "kubectl {} exited with {}: {}",
            args.join(" "),
            output.status,
            stderr.trim()
        );
        return Err(stderr.trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
