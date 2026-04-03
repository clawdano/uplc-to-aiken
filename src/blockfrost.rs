use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ScriptInfo {
    pub script_hash: String,
    #[serde(rename = "type")]
    pub script_type: String,
    pub serialised_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ScriptCbor {
    pub cbor: Option<String>,
}

pub struct BlockfrostClient {
    api_key: String,
    base_url: String,
    client: reqwest::blocking::Client,
}

impl BlockfrostClient {
    pub fn new(api_key: &str, network: &str) -> Self {
        let base_url = match network {
            "mainnet" => "https://cardano-mainnet.blockfrost.io/api/v0".to_string(),
            "preprod" => "https://cardano-preprod.blockfrost.io/api/v0".to_string(),
            "preview" => "https://cardano-preview.blockfrost.io/api/v0".to_string(),
            other => panic!("Unknown network: {}", other),
        };

        Self {
            api_key: api_key.to_string(),
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    /// List scripts from the chain (paginated)
    pub fn list_scripts(&self, page: u32) -> Result<Vec<ScriptInfo>, String> {
        // First get script hashes
        let url = format!("{}/scripts?page={}&count=100&order=desc", self.base_url, page);
        let resp: Vec<serde_json::Value> = self
            .client
            .get(&url)
            .header("project_id", &self.api_key)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .map_err(|e| format!("JSON parse failed: {}", e))?;

        let mut scripts = Vec::new();
        for item in resp {
            if let Some(hash) = item["script_hash"].as_str() {
                // Get script details
                match self.get_script_info(hash) {
                    Ok(info) => scripts.push(info),
                    Err(_) => continue,
                }
            }
        }

        Ok(scripts)
    }

    /// Get script info by hash
    pub fn get_script_info(&self, hash: &str) -> Result<ScriptInfo, String> {
        let url = format!("{}/scripts/{}", self.base_url, hash);
        self.client
            .get(&url)
            .header("project_id", &self.api_key)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?
            .json::<ScriptInfo>()
            .map_err(|e| format!("JSON parse failed: {}", e))
    }

    /// Get script CBOR by hash
    pub fn get_script_cbor(&self, hash: &str) -> Result<String, String> {
        let url = format!("{}/scripts/{}/cbor", self.base_url, hash);
        let resp: ScriptCbor = self
            .client
            .get(&url)
            .header("project_id", &self.api_key)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?
            .json()
            .map_err(|e| format!("JSON parse failed: {}", e))?;

        resp.cbor.ok_or_else(|| "No CBOR data for this script".to_string())
    }

    /// Fetch Plutus V2 scripts from the chain
    pub fn fetch_plutus_v2_scripts(&self, count: usize) -> Result<Vec<(String, String)>, String> {
        let mut results = Vec::new();
        let mut page = 1;

        while results.len() < count && page <= 10 {
            let scripts = self.list_scripts(page)?;
            if scripts.is_empty() {
                break;
            }

            for script in scripts {
                if script.script_type == "plutusV2" {
                    match self.get_script_cbor(&script.script_hash) {
                        Ok(cbor) => {
                            results.push((script.script_hash, cbor));
                            if results.len() >= count {
                                break;
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }

            page += 1;
        }

        Ok(results)
    }
}
