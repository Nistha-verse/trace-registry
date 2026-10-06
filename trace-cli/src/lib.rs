use {
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    reqwest::blocking::Client as HttpClient,
    serde::{Deserialize, Serialize},
    serde_json::{json, Value},
    solana_pubkey::Pubkey,
    std::{fmt, fs::OpenOptions, io::Write, path::Path, str::FromStr},
};

pub const DEFAULT_TRACE_PROGRAM_ID: &str = "H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w";

#[derive(Debug)]
pub enum ClientError {
    InvalidPubkey(String),
    Http(reqwest::Error),
    Json(serde_json::Error),
    Rpc(String),
    InvalidAccountData(String),
    Io(std::io::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPubkey(value) => write!(f, "invalid public key: {value}"),
            Self::Http(error) => write!(f, "RPC request failed: {error}"),
            Self::Json(error) => write!(f, "invalid RPC JSON: {error}"),
            Self::Rpc(error) => write!(f, "RPC error: {error}"),
            Self::InvalidAccountData(error) => write!(f, "invalid Trace account data: {error}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<reqwest::Error> for ClientError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone)]
pub struct TraceClient {
    http: HttpClient,
    rpc_url: String,
    trace_program: Pubkey,
}

impl TraceClient {
    pub fn new(rpc_url: impl Into<String>, trace_program: &str) -> Result<Self, ClientError> {
        let trace_program = Pubkey::from_str(trace_program)
            .map_err(|_| ClientError::InvalidPubkey(trace_program.to_owned()))?;
        Ok(Self {
            http: HttpClient::new(),
            rpc_url: rpc_url.into(),
            trace_program,
        })
    }

    pub fn attestation_pda(&self, target: &Pubkey) -> Pubkey {
        Pubkey::derive_program_address(&[b"attestation", target.as_ref()], &self.trace_program)
            .expect("Trace PDA derivation failed")
            .0
    }

    pub fn dependency_manifest_pda(&self, parent: &Pubkey) -> Pubkey {
        Pubkey::derive_program_address(&[b"dependencies", parent.as_ref()], &self.trace_program)
            .expect("Trace PDA derivation failed")
            .0
    }

    pub fn verify(&self, target: &str) -> Result<ProgramReport, ClientError> {
        let target = parse_pubkey(target)?;
        let attestation_address = self.attestation_pda(&target);
        let manifest_address = self.dependency_manifest_pda(&target);
        let attestation = self
            .get_account(&attestation_address)?
            .map(|data| decode_attestation(&data))
            .transpose()?;
        let dependencies = self
            .get_account(&manifest_address)?
            .map(|data| decode_dependency_manifest(&data))
            .transpose()?;

        Ok(ProgramReport {
            target_program: target.to_string(),
            trace_program: self.trace_program.to_string(),
            attestation_address: attestation_address.to_string(),
            attestation,
            dependency_manifest_address: manifest_address.to_string(),
            dependencies,
        })
    }

    pub fn history(&self, target: &str, limit: usize) -> Result<Vec<HistoryRecord>, ClientError> {
        let target = parse_pubkey(target)?;
        let address = self.attestation_pda(&target);
        let signatures: RpcResponse<Vec<SignatureInfo>> = self.rpc(
            "getSignaturesForAddress",
            json!([address.to_string(), {"limit": limit.min(1_000)}]),
        )?;
        let mut events = Vec::new();
        for signature in signatures.result {
            let transaction: RpcResponse<Option<TransactionInfo>> = self.rpc(
                "getTransaction",
                json!([
                    signature.signature,
                    {"encoding": "json", "maxSupportedTransactionVersion": 0}
                ]),
            )?;
            let Some(transaction) = transaction.result else {
                continue;
            };
            let Some(meta) = transaction.meta else {
                continue;
            };
            for log in meta.log_messages.unwrap_or_default() {
                let Some(encoded) = log.strip_prefix("Program data: ") else {
                    continue;
                };
                let data = BASE64.decode(encoded).map_err(|error| {
                    ClientError::InvalidAccountData(format!("invalid event base64: {error}"))
                })?;
                if let Some(event) = decode_event(&data, &signature.signature, transaction.slot) {
                    events.push(event);
                }
            }
        }
        Ok(events)
    }

    fn get_account(&self, address: &Pubkey) -> Result<Option<Vec<u8>>, ClientError> {
        let response: RpcResponse<Option<RpcAccount>> = self.rpc(
            "getAccountInfo",
            json!([address.to_string(), {"encoding": "base64"}]),
        )?;
        response
            .result
            .map(|account| {
                let encoded = account
                    .data
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ClientError::InvalidAccountData("missing base64 account data".into())
                    })?;
                BASE64.decode(encoded).map_err(|error| {
                    ClientError::InvalidAccountData(format!("invalid base64 data: {error}"))
                })
            })
            .transpose()
    }

    fn rpc<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<RpcResponse<T>, ClientError> {
        let response: RpcEnvelope<T> = self
            .http
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()?
            .error_for_status()?
            .json()?;
        if let Some(error) = response.error {
            return Err(ClientError::Rpc(error.to_string()));
        }
        response
            .result
            .map(|result| RpcResponse { result })
            .ok_or_else(|| ClientError::Rpc("RPC response omitted result".into()))
    }
}

#[derive(Debug, Serialize)]
pub struct ProgramReport {
    pub target_program: String,
    pub trace_program: String,
    pub attestation_address: String,
    pub attestation: Option<AttestationView>,
    pub dependency_manifest_address: String,
    pub dependencies: Option<DependencyManifestView>,
}

#[derive(Debug, Serialize)]
pub struct AttestationView {
    pub target_program: String,
    pub program_data: Option<String>,
    pub authority: Option<String>,
    pub risk: String,
    pub multisig_config: Option<String>,
    pub multisig_threshold: u8,
    pub multisig_member_count: u8,
    pub loader: String,
    pub last_deploy_slot: u64,
    pub attested_slot: u64,
    pub attested_at: i64,
    pub attester: String,
    pub refresh_count: u32,
}

#[derive(Debug, Serialize)]
pub struct DependencyManifestView {
    pub target_program: String,
    pub dependencies: Vec<DependencyView>,
    pub last_observed_slot: u64,
}

#[derive(Debug, Serialize)]
pub struct DependencyView {
    pub program_id: String,
    pub risk: String,
    pub attested_slot: u64,
    pub first_observed_slot: u64,
    pub last_observed_slot: u64,
}

#[derive(Debug, Serialize)]
pub struct HistoryRecord {
    pub signature: String,
    pub slot: u64,
    pub kind: String,
    pub target_program: String,
    pub authority: Option<String>,
    pub old_authority: Option<String>,
    pub new_authority: Option<String>,
    pub dependency_program: Option<String>,
    pub risk: Option<String>,
    pub attested_slot: Option<u64>,
    pub refresh_count: Option<u32>,
}

pub fn append_history(
    path: impl AsRef<Path>,
    records: &[HistoryRecord],
) -> Result<(), ClientError> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for record in records {
        writeln!(file, "{}", serde_json::to_string(record)?)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<Value>,
}

struct RpcResponse<T> {
    result: T,
}

#[derive(Debug, Deserialize)]
struct RpcAccount {
    data: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct SignatureInfo {
    signature: String,
}

#[derive(Debug, Deserialize)]
struct TransactionInfo {
    slot: u64,
    meta: Option<TransactionMeta>,
}

#[derive(Debug, Deserialize)]
struct TransactionMeta {
    #[serde(rename = "logMessages")]
    log_messages: Option<Vec<String>>,
}

fn parse_pubkey(value: &str) -> Result<Pubkey, ClientError> {
    Pubkey::from_str(value).map_err(|_| ClientError::InvalidPubkey(value.to_owned()))
}

fn read_bytes<'a>(
    data: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], ClientError> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| ClientError::InvalidAccountData("offset overflow".into()))?;
    let value = data
        .get(*offset..end)
        .ok_or_else(|| ClientError::InvalidAccountData("account data is truncated".into()))?;
    *offset = end;
    Ok(value)
}

fn read_u8(data: &[u8], offset: &mut usize) -> Result<u8, ClientError> {
    Ok(read_bytes(data, offset, 1)?[0])
}

fn read_u32(data: &[u8], offset: &mut usize) -> Result<u32, ClientError> {
    Ok(u32::from_le_bytes(
        read_bytes(data, offset, 4)?.try_into().unwrap(),
    ))
}

fn read_u64(data: &[u8], offset: &mut usize) -> Result<u64, ClientError> {
    Ok(u64::from_le_bytes(
        read_bytes(data, offset, 8)?.try_into().unwrap(),
    ))
}

fn read_i64(data: &[u8], offset: &mut usize) -> Result<i64, ClientError> {
    Ok(i64::from_le_bytes(
        read_bytes(data, offset, 8)?.try_into().unwrap(),
    ))
}

fn read_pubkey(data: &[u8], offset: &mut usize) -> Result<Pubkey, ClientError> {
    Ok(Pubkey::new_from_array(
        read_bytes(data, offset, 32)?.try_into().unwrap(),
    ))
}

fn read_optional_pubkey(data: &[u8], offset: &mut usize) -> Result<Option<Pubkey>, ClientError> {
    match read_u8(data, offset)? {
        0 => Ok(None),
        1 => Ok(Some(read_pubkey(data, offset)?)),
        flag => Err(ClientError::InvalidAccountData(format!(
            "invalid option flag {flag}"
        ))),
    }
}

fn decode_attestation(data: &[u8]) -> Result<AttestationView, ClientError> {
    let mut offset = 8;
    let target_program = read_pubkey(data, &mut offset)?;
    let program_data = read_optional_pubkey(data, &mut offset)?;
    let authority = read_optional_pubkey(data, &mut offset)?;
    let risk = match read_u8(data, &mut offset)? {
        0 => "immutable",
        1 => "multisig_protected",
        2 => "program_controlled",
        3 => "single_key",
        value => {
            return Err(ClientError::InvalidAccountData(format!(
                "invalid risk {value}"
            )))
        }
    };
    let multisig_config = read_optional_pubkey(data, &mut offset)?;
    let multisig_threshold = read_u8(data, &mut offset)?;
    let multisig_member_count = read_u8(data, &mut offset)?;
    let loader = match read_u8(data, &mut offset)? {
        0 => "upgradeable",
        1 => "legacy",
        2 => "loader_v4",
        value => {
            return Err(ClientError::InvalidAccountData(format!(
                "invalid loader {value}"
            )))
        }
    };
    let last_deploy_slot = read_u64(data, &mut offset)?;
    let attested_slot = read_u64(data, &mut offset)?;
    let attested_at = read_i64(data, &mut offset)?;
    let attester = read_pubkey(data, &mut offset)?;
    let refresh_count = read_u32(data, &mut offset)?;
    let _bump = read_u8(data, &mut offset)?;
    Ok(AttestationView {
        target_program: target_program.to_string(),
        program_data: program_data.map(|value| value.to_string()),
        authority: authority.map(|value| value.to_string()),
        risk: risk.into(),
        multisig_config: multisig_config.map(|value| value.to_string()),
        multisig_threshold,
        multisig_member_count,
        loader: loader.into(),
        last_deploy_slot,
        attested_slot,
        attested_at,
        attester: attester.to_string(),
        refresh_count,
    })
}

fn decode_dependency_manifest(data: &[u8]) -> Result<DependencyManifestView, ClientError> {
    let mut offset = 8;
    let target_program = read_pubkey(data, &mut offset)?;
    let count = read_u32(data, &mut offset)? as usize;
    let mut dependencies = Vec::with_capacity(count);
    for _ in 0..count {
        let program_id = read_pubkey(data, &mut offset)?;
        let risk = match read_u8(data, &mut offset)? {
            0 => "immutable",
            1 => "multisig_protected",
            2 => "program_controlled",
            3 => "single_key",
            value => {
                return Err(ClientError::InvalidAccountData(format!(
                    "invalid dependency risk {value}"
                )))
            }
        };
        dependencies.push(DependencyView {
            program_id: program_id.to_string(),
            risk: risk.into(),
            attested_slot: read_u64(data, &mut offset)?,
            first_observed_slot: read_u64(data, &mut offset)?,
            last_observed_slot: read_u64(data, &mut offset)?,
        });
    }
    Ok(DependencyManifestView {
        target_program: target_program.to_string(),
        dependencies,
        last_observed_slot: read_u64(data, &mut offset)?,
    })
}

fn decode_event(data: &[u8], signature: &str, slot: u64) -> Option<HistoryRecord> {
    const ATTESTED: [u8; 8] = [184, 102, 113, 199, 220, 197, 96, 50];
    const AUTHORITY_CHANGED: [u8; 8] = [31, 19, 174, 152, 4, 82, 215, 226];
    const DEPENDENCY_OBSERVED: [u8; 8] = [161, 106, 112, 18, 102, 43, 192, 197];
    let mut offset = 8;
    let target_program = read_pubkey(data, &mut offset).ok()?.to_string();

    if data.starts_with(&ATTESTED) {
        let authority = read_optional_pubkey(data, &mut offset)
            .ok()?
            .map(|value| value.to_string());
        let risk = read_risk(data, &mut offset).ok();
        let _multisig_config = read_optional_pubkey(data, &mut offset).ok()?;
        let _loader = read_u8(data, &mut offset).ok()?;
        let attested_slot = read_u64(data, &mut offset).ok()?;
        let refresh_count = read_u32(data, &mut offset).ok()?;
        return Some(HistoryRecord {
            signature: signature.into(),
            slot,
            kind: "attested".into(),
            target_program,
            authority,
            old_authority: None,
            new_authority: None,
            dependency_program: None,
            risk,
            attested_slot: Some(attested_slot),
            refresh_count: Some(refresh_count),
        });
    }

    if data.starts_with(&AUTHORITY_CHANGED) {
        let old_authority = read_optional_pubkey(data, &mut offset)
            .ok()?
            .map(|value| value.to_string());
        let new_authority = read_optional_pubkey(data, &mut offset)
            .ok()?
            .map(|value| value.to_string());
        return Some(HistoryRecord {
            signature: signature.into(),
            slot,
            kind: "authority_changed".into(),
            target_program,
            authority: None,
            old_authority,
            new_authority,
            dependency_program: None,
            risk: None,
            attested_slot: None,
            refresh_count: None,
        });
    }

    if data.starts_with(&DEPENDENCY_OBSERVED) {
        let dependency_program = read_pubkey(data, &mut offset).ok()?.to_string();
        let risk = read_risk(data, &mut offset).ok();
        return Some(HistoryRecord {
            signature: signature.into(),
            slot,
            kind: "dependency_observed".into(),
            target_program,
            authority: None,
            old_authority: None,
            new_authority: None,
            dependency_program: Some(dependency_program),
            risk,
            attested_slot: None,
            refresh_count: None,
        });
    }

    None
}

fn read_risk(data: &[u8], offset: &mut usize) -> Result<String, ClientError> {
    Ok(match read_u8(data, offset)? {
        0 => "immutable",
        1 => "multisig_protected",
        2 => "program_controlled",
        3 => "single_key",
        value => {
            return Err(ClientError::InvalidAccountData(format!(
                "invalid risk {value}"
            )))
        }
    }
    .into())
}
