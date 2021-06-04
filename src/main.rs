use attohttpc::header;
use serde::Deserialize;
use std::env;
use std::str::FromStr;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const READING_FAILED: &str = "Reading file failed: ";
const CONVERSION_FAILED: &str = "Bytes to string conversion failed: ";

#[derive(Deserialize)]
struct ResponseWithId {
	pub id: u64,
}

fn optional_env_var(env_var: &'static str) -> Option<String> {
	match env::var(env_var) {
		Ok(checksums_string) => Some(checksums_string),
		Err(e) => match e {
			env::VarError::NotPresent => None,
			_ => panic!("Couldn't resolve checksums setting: {:?}", e),
		},
	}
}

fn auth_request(req: attohttpc::RequestBuilder, api_key: &str) -> attohttpc::RequestBuilder {
	let user_agent = "drone-plugin-gitea/".to_owned() + VERSION;

	req.header(header::USER_AGENT, &user_agent).bearer_auth(api_key)
}

fn filename_to_contents(filename: String) -> String {
	let bytes = std::fs::read(&filename).expect(&(READING_FAILED.to_owned() + &filename));
	String::from_utf8(bytes).expect(&(CONVERSION_FAILED.to_owned() + &filename))
}

fn main() {
	// Drone tag name
	let tag_name = env::var("DRONE_TAG").expect("DRONE_TAG to be set");

	// Compute the releases api endpoint
	let api_url: String = env::var("PLUGIN_BASE_URL")
		.expect("BASE_URL is not set properly")
		.trim_end_matches('/')
		.to_owned()
		+ "/api/v1/repos/"
		+ &env::var("DRONE_REPO").expect("DRONE_REPO is not set properly")
		+ "/releases";

	// ApiKey
	let api_key = env::var("PLUGIN_API_KEY").expect("API_KEY is not set properly");

	// Text content from files
	let name = optional_env_var("PLUGIN_NAME").map(filename_to_contents);
	let body = optional_env_var("PLUGIN_BODY").map(filename_to_contents);

	// Booleans
	let is_draft =
		optional_env_var("PLUGIN_DRAFT").map(|s| bool::from_str(&s).expect("DRAFT is not a valid boolean"));
	let is_prerelease = optional_env_var("PLUGIN_PRERELEASE")
		.map(|s| bool::from_str(&s).expect("PRERELEASE is not a valid boolean"));

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

	assert!(res.is_success());

	let res_json: ResponseWithId = res
		.json()
		.expect("parsing ID from release creation response json failed");

	if let Some(assets) = optional_env_var("PLUGIN_ASSETS") {
		let assets_api_url: String = api_url + "/" + &res_json.id.to_string() + "/assets";

		for asset_filename in assets.split(',') {
			let asset_file =
				std::fs::File::open(&asset_filename).expect(&(READING_FAILED.to_owned() + asset_filename));

			// We wanna validate that we got the ID for the attachment, aka it pretty surely succeeded.
			let _res: ResponseWithId = auth_request(attohttpc::post(&assets_api_url), &api_key)
				.file(asset_file)
				.send()
				.expect(&("asset uploading failed for file ".to_owned() + asset_filename))
				.json()
				.expect("parsing ID from asset creation response json failed");
		}
	}
}
