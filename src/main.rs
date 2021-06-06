#![deny(clippy::all)]
#![warn(clippy::pedantic)]

use attohttpc::header;
use glob::glob;
use serde::Deserialize;
use std::env;
use std::str::FromStr;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const READING_FILE_FAILED: &str = "Reading file failed: ";
const GLOB_FAILED: &str = "Parsing glob failed: ";
const CONVERSION_FAILED: &str = "Bytes to string conversion failed: ";

#[derive(Deserialize)]
struct ResponseWithId {
	pub id: u64,
}

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

fn auth_request(req: attohttpc::RequestBuilder, api_key: &str) -> attohttpc::RequestBuilder {
	let user_agent = "drone-plugin-gitea/".to_owned() + VERSION;

	req.header(header::USER_AGENT, &user_agent).bearer_auth(api_key)
}

fn filename_to_contents(filename: &str) -> String {
	let bytes = std::fs::read(&filename).expect(&(READING_FILE_FAILED.to_owned() + filename));
	String::from_utf8(bytes).expect(&(CONVERSION_FAILED.to_owned() + filename))
}

fn main() {
	// Drone info
	let tag_name = env::var("DRONE_TAG").expect("DRONE_TAG to be set");
	let owner_and_repo = env::var("DRONE_REPO").expect("DRONE_REPO is not set properly");

	// Apikey is always required without any fallback
	let api_key = env::var("PLUGIN_API_KEY").expect("setting api_key is not set properly");

	let base_url = optional_env_var("PLUGIN_BASE_URL").unwrap_or_else(|| {
		// Use the repo's owner/name and it's link to calculate a base_url default
		env::var("DRONE_REPO_LINK")
			.ok()
			.and_then(|v| {
				v.strip_suffix(&owner_and_repo)
					.map(std::borrow::ToOwned::to_owned)
			})
			.expect("setting base_url & DRONE_REPO_LINK are not set properly")
	});

	// Compute the releases api endpoint
	let api_url: String =
		base_url.trim_end_matches('/').to_owned() + "/api/v1/repos/" + &owner_and_repo + "/releases";

	// Text content from files
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

	let release_create_json = serde_json::json!({
		"tag_name": tag_name,
		"name": name,
		"body": body,
		"prerelease": is_prerelease,
		"draft": is_draft
	});

	let res = auth_request(
		attohttpc::post(&api_url).param(header::CONTENT_TYPE, "application/json"),
		&api_key,
	)
	.json(&release_create_json)
	.expect("release creation json payload parsing failed")
	.send()
	.expect("release creation request failed");

	assert!(res.is_success(), "release creation request wasn't a success");

	let res_json: ResponseWithId = res
		.json()
		.expect("parsing ID from release creation response json failed");

	if let Some(asset_globs) = optional_env_var("PLUGIN_ASSETS") {
		let assets_api_url: String = api_url + "/" + &res_json.id.to_string() + "/assets";

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

		for asset_filename in assets {
			let asset_file = std::fs::File::open(&asset_filename)
				.expect(&(READING_FILE_FAILED.to_owned() + &asset_filename.to_string_lossy()));

			// We wanna validate that we got the ID for the attachment, aka it pretty surely succeeded.
			let res = auth_request(attohttpc::post(&assets_api_url), &api_key)
				.file(asset_file)
				.send()
				.expect(&("asset uploading failed for file ".to_owned() + &asset_filename.to_string_lossy()));

			if !res.is_success() {
				panic!(
					"asset uploading failed for file: {}, status code is: {}",
					&asset_filename.to_string_lossy(),
					res.status().as_str()
				);
			}
		}
	}
}
