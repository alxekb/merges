use anyhow::{Context, Result};
use octocrab::Octocrab;
use serde::Deserialize;

/// Build an authenticated Octocrab client.
pub fn client(token: &str) -> Result<Octocrab> {
    Octocrab::builder()
        .personal_token(token.to_string())
        .build()
        .context("Failed to build GitHub client")
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // fields used selectively; kept for future display/serialisation
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: String,
    pub ci_status: String,
    pub review_state: String,
}

/// Create a pull request; returns (pr_number, pr_url).
pub async fn create_pr(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    title: &str,
    head: &str,
    base: &str,
    body: &str,
) -> Result<(u64, String)> {
    let pr = client
        .pulls(owner, repo)
        .create(title, head, base)
        .body(body)
        .send()
        .await
        .with_context(|| format!("Failed to create PR '{}'", title))?;

    let number = pr.number;
    let url = pr
        .html_url
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("https://github.com/{}/{}/pull/{}", owner, repo, number));

    Ok((number, url))
}

/// Update the base branch of an existing PR (used in stacked mode after lower chunk merges).
pub async fn update_pr_base(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    new_base: &str,
) -> Result<()> {
    client
        .pulls(owner, repo)
        .update(pr_number)
        .base(new_base)
        .send()
        .await
        .with_context(|| format!("Failed to update base for PR #{}", pr_number))?;
    Ok(())
}

/// Retrieve basic PR info including CI and review state.
pub async fn get_pr_info(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<PrInfo> {
    let pr = client
        .pulls(owner, repo)
        .get(pr_number)
        .await
        .with_context(|| format!("Failed to fetch PR #{}", pr_number))?;

    let state = pr
        .state
        .map(|s| format!("{:?}", s).to_lowercase())
        .unwrap_or_else(|| "unknown".to_string());

    let url = pr
        .html_url
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("https://github.com/{}/{}/pull/{}", owner, repo, pr_number));

    let title = pr.title.unwrap_or_default();

    // Fetch combined commit status
    let ci_status = get_ci_status(client, owner, repo, pr_number).await.unwrap_or_else(|_| "unknown".to_string());
    let review_state = get_review_state(client, owner, repo, pr_number).await.unwrap_or_else(|_| "unknown".to_string());

    Ok(PrInfo {
        number: pr_number,
        url,
        title,
        state,
        ci_status,
        review_state,
    })
}

async fn get_ci_status(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<String> {
    // Get the PR's head SHA via the checks API
    let pr = client.pulls(owner, repo).get(pr_number).await?;
    let sha = pr
        .head
        .sha;

    #[derive(Deserialize)]
    struct CombinedStatus {
        state: String,
    }

    let combined: CombinedStatus = client
        .get(
            format!("/repos/{}/{}/commits/{}/status", owner, repo, sha),
            None::<&()>,
        )
        .await
        .unwrap_or(CombinedStatus { state: "pending".to_string() });

    Ok(combined.state)
}

async fn get_review_state(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<String> {
    #[derive(Deserialize)]
    struct Review {
        state: String,
    }

    let reviews: Vec<Review> = client
        .get(
            format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_number),
            None::<&()>,
        )
        .await
        .unwrap_or_default();

    // Determine overall review state from the latest review per reviewer
    if reviews.iter().any(|r| r.state == "APPROVED") {
        Ok("approved".to_string())
    } else if reviews.iter().any(|r| r.state == "CHANGES_REQUESTED") {
        Ok("changes_requested".to_string())
    } else if reviews.is_empty() {
        Ok("pending".to_string())
    } else {
        Ok("commented".to_string())
    }
}
