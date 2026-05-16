use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub device_id: String,
    pub device_name: String,
    pub public_key: Option<String>,
}

impl Identity {
    #[allow(dead_code)]
    pub fn new(device_id: String, device_name: String) -> Self {
        Self {
            device_id,
            device_name,
            public_key: None,
        }
    }
}
