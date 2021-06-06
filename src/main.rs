//! A tool to be ran in a drone CI system to create a release in gitea and upload assets to it.
//!
//! Error handling is mostly done via calls to panic!, as this is such a small purpose built tool
//! Contributions are welcome though, as long as they aren't removing the core functionality this tool was built for

#![deny(clippy::all)]
#![warn(clippy::pedantic)]

use attohttpc::header;
use glob::glob;
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::str::FromStr;

// Used in User Agent
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Error messages
const READING_FILE_FAILED: &str = "Reading file failed: ";
const GLOB_FAILED: &str = "Parsing glob failed: ";
const CONVERSION_FAILED: &str = "Bytes to string conversion failed: ";

/// Part of a response from gitea when a release was created succesfully.
///
/// No need to parse everything, as in the future deprecations could mean more work.
#[derive(Deserialize, Debug)]
struct ReleaseCreatedResponse {
	pub id: u64,
	pub url: String,
}

/// Gets an environment variable or None if it does not exist.
///
/// Panics on other errors than the env var not existing.
fn optional_env_var(env_var: &'static str) -> Option<String> {
	match env::var(env_var) {
		Ok(checksums_string) => Some(checksums_string),
		Err(e) => {
			if e == env::VarError::NotPresent {
				None
			} else {
				panic!("Couldn't resolve checksums setting: {:?}", e);
			}
		}
	}
}

/// Adds common things to a request, such as authentication and the user agent.
fn auth_request(req: attohttpc::RequestBuilder, api_key: &str) -> attohttpc::RequestBuilder {
	let user_agent = "drone-plugin-gitea/".to_owned() + VERSION;

	req.header(header::USER_AGENT, &user_agent)
		.header(header::ACCEPT, "application/json")
		.bearer_auth(api_key)
}

/// Reads a file from filename to a String. Panics on errors.
fn filename_to_contents(filename: &str) -> String {
	let bytes = std::fs::read(&filename).expect(&(READING_FILE_FAILED.to_owned() + filename));
	String::from_utf8(bytes).expect(&(CONVERSION_FAILED.to_owned() + filename))
}

fn main() {
	// Drone pipeline provided env vars.
	let tag_name = env::var("DRONE_TAG").expect("DRONE_TAG to be set");
	let owner_and_repo = env::var("DRONE_REPO").expect("DRONE_REPO is not set properly");

	// Apikey is always required without any fallback
	let api_key = env::var("PLUGIN_API_KEY").expect("setting api_key is not set properly");

	// Prioritize user provided base_url setting
	let base_url = optional_env_var("PLUGIN_BASE_URL").unwrap_or_else(|| {
		// ...but if it's not provided try to calculate it from Drone provided env.
		env::var("DRONE_REPO_LINK")
			.ok()
			.and_then(|v| {
				v.strip_suffix(&owner_and_repo)
					.map(std::borrow::ToOwned::to_owned)
			})
			.expect("setting base_url & DRONE_REPO_LINK are not set properly")
	});

	// The URL to POST to to create a new release.
	let api_url: String =
		base_url.trim_end_matches('/').to_owned() + "/api/v1/repos/" + &owner_and_repo + "/releases";

	// Text content from files. Can also be missing.
	let name = optional_env_var("PLUGIN_NAME")
		.map(|filename| filename_to_contents(&filename))
		.map(|contents| contents.trim().to_owned());
	let body = optional_env_var("PLUGIN_BODY")
		.map(|filename| filename_to_contents(&filename))
		.map(|contents| contents.trim().to_owned());

	// Booleans
	let is_draft = optional_env_var("PLUGIN_DRAFT")
		.map(|s| bool::from_str(&s).expect("setting draft is not a valid boolean"));
	let is_prerelease = optional_env_var("PLUGIN_PRERELEASE")
		.map(|s| bool::from_str(&s).expect("setting prerelease is not a valid boolean"));

	// Release creation JSON payload
	let release_create_json = serde_json::json!({
		"tag_name": tag_name,
		"name": name,
		"body": body,
		"prerelease": is_prerelease,
		"draft": is_draft
	});

	// Release creation request response
	let res = auth_request(
		attohttpc::post(&api_url).param(header::CONTENT_TYPE, "application/json"),
		&api_key,
	)
	.json(&release_create_json)
	.expect("release creation json payload parsing failed")
	.send()
	.expect("release creation request failed");

	if !res.is_success() {
		panic!(
			"release creation request wasn't a success with status {} and response: {:?}",
			res.status(),
			res.json::<Value>()
		);
	}

	let res_json: ReleaseCreatedResponse = res.json().expect("parsing release creation response json failed");
	println!("Successfully created release: {}", &res_json.url);

	// If there are assets to upload, iterate over the globs.
	if let Some(asset_globs) = optional_env_var("PLUGIN_ASSETS") {
		// The URL to POST to to create a new asset for the release.
		let assets_api_url: String = api_url + "/" + &res_json.id.to_string() + "/assets";

		// List of the files gotten from processing trough the globs
		let mut assets = vec![];

		// Process globs into all paths
		for asset_glob in asset_globs.split(',') {
			let paths = glob(&asset_glob).expect(&(GLOB_FAILED.to_owned() + asset_glob));
			for path in paths {
				let filepath = path.expect(&(READING_FILE_FAILED.to_owned() + asset_glob));
				if filepath.is_file() {
					assets.push(filepath);
				}
			}
		}

		// Iterate over the filepaths gotten from the globs.
		for asset_path in assets {
			// The filename to give to gitea for the file, or for debugging errors.
			let asset_filename = asset_path.to_string_lossy();

			let asset_file_contents =
				std::fs::read(&asset_path).expect(&("reading asset failed".to_owned() + &asset_filename));

			let res = auth_request(attohttpc::post(&assets_api_url), &api_key)
				.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
				.param("name", &asset_filename)
				.bytes(&asset_file_contents)
				.send()
				.expect(&("uploading failed for asset ".to_owned() + &asset_filename));

			// Even a single failed asset will fail the entire step.
			if !res.is_success() {
				panic!(
					"asset uploading failed for file: {} with an status {} and response: {:?}",
					&asset_filename,
					res.status(),
					res.json::<Value>()
				);
			}
		}
	}
}
