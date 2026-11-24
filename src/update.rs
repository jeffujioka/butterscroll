#![allow(dead_code)] // the update check is wired up by a later task
//! Latest-release lookup and the "is there an update to offer" decision.

use serde::Deserialize;

use crate::version::Version;

/// The GitHub repository that publishes butterscroll releases. The one place to rename.
pub const REPO: &str = "jeffujioka/butterscroll";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub zip_url: String,
    pub page_url: String,
}

/// The subset of GitHub's `releases/latest` response that the update check reads.
#[derive(Deserialize)]
struct LatestJson {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// The universal zip that a release of `v` must attach.
pub fn asset_name(v: &Version) -> String {
    format!("butterscroll-v{v}-universal.zip")
}

/// `override_url` if given, else the GitHub API endpoint for the latest release of [`REPO`].
pub fn latest_url_from(override_url: Option<&str>) -> String {
    match override_url {
        Some(url) => url.to_owned(),
        None => format!("https://api.github.com/repos/{REPO}/releases/latest"),
    }
}

/// [`latest_url_from`] with the `BUTTERSCROLL_UPDATE_URL` environment override.
pub fn latest_url() -> String {
    latest_url_from(std::env::var("BUTTERSCROLL_UPDATE_URL").ok().as_deref())
}

/// Reads a `releases/latest` response. Errors on non-release bodies, drafts, pre-releases,
/// unparsable tags and releases without the universal zip.
pub fn parse_latest(json: &str) -> Result<Release, String> {
    let latest: LatestJson =
        serde_json::from_str(json).map_err(|e| format!("bad release JSON: {e}"))?;
    if latest.draft || latest.prerelease {
        return Err(format!("release {} is a draft or pre-release", latest.tag_name));
    }
    let version = Version::from_tag(&latest.tag_name)
        .ok_or_else(|| format!("bad release tag {:?}", latest.tag_name))?;
    let wanted = asset_name(&version);
    let asset = latest
        .assets
        .iter()
        .find(|a| a.name == wanted)
        .ok_or_else(|| format!("release {} has no {wanted} asset", latest.tag_name))?;
    Ok(Release {
        version,
        tag: latest.tag_name.clone(),
        zip_url: asset.browser_download_url.clone(),
        page_url: latest.html_url,
    })
}

/// True when `latest` is a final release strictly newer than `running`.
pub fn is_offer(running: &Version, latest: &Release) -> bool {
    latest.version.pre.is_none() && latest.version > *running
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_latest_reads_tag_and_asset() {
        let r = parse_latest(include_str!("fixtures/release-latest.json")).unwrap();
        assert_eq!(r.tag, "v0.2.0");
        assert_eq!(r.version, Version::parse("0.2.0").unwrap());
        assert!(r.zip_url.ends_with("/butterscroll-v0.2.0-universal.zip"));
    }
    #[test]
    fn parse_latest_requires_matching_asset() {
        // Same release, but the universal zip is renamed: no asset matches `asset_name`.
        let json = include_str!("fixtures/release-latest.json")
            .replace("butterscroll-v0.2.0-universal.zip", "butterscroll-v0.2.0-arm64.zip");
        assert!(parse_latest(&json).is_err());
    }
    #[test]
    fn parse_latest_rejects_error_body() {
        assert!(parse_latest(include_str!("fixtures/rate-limited.json")).is_err());
    }
    #[test]
    fn parse_latest_rejects_garbage() {
        assert!(parse_latest("<html>").is_err());
    }
    #[test]
    fn offers_only_newer_finals() {
        let v = |s: &str| Version::parse(s).unwrap();
        let release = |s: &str| Release {
            version: v(s),
            tag: format!("v{s}"),
            zip_url: String::new(),
            page_url: String::new(),
        };
        // running 0.1.0
        assert!(is_offer(&v("0.1.0"), &release("0.2.0")));
        assert!(!is_offer(&v("0.1.0"), &release("0.1.0")));
        assert!(!is_offer(&v("0.1.0"), &release("0.0.9")));
        assert!(!is_offer(&v("0.1.0"), &release("0.2.0-rc.1")));
        // running 0.2.0-rc.1
        assert!(is_offer(&v("0.2.0-rc.1"), &release("0.2.0")));
    }
    #[test]
    fn update_url_override() {
        assert_eq!(latest_url_from(Some("http://x/l.json")), "http://x/l.json");
        assert!(latest_url_from(None).ends_with("/repos/jeffujioka/butterscroll/releases/latest"));
    }
}
