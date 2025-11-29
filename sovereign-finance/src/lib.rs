use bdk::bitcoin::blockdata::script::Instruction;
use bdk::bitcoin::Address;
use bdk::blockchain::{ElectrumBlockchain, GetTx};
use bdk::electrum_client::Client;
use sha2::{Sha256, Digest};
use std::str::FromStr;
use log::{info, warn};

pub struct LicenseVerifier {
    blockchain: ElectrumBlockchain,
}

impl LicenseVerifier {
    pub fn new(electrum_url: &str) -> anyhow::Result<Self> {
        let client = Client::new(electrum_url)?;
        Ok(Self { blockchain: ElectrumBlockchain::from(client) })
    }

    /// Verifies a machine-locked license on the Bitcoin blockchain.
    pub fn verify_license(
        &self,
        txid_str: &str,
        machine_id: &str,
        developer_addr: &str,
        required_sats: u64,
    ) -> anyhow::Result<bool> {
        let txid = bdk::bitcoin::Txid::from_str(txid_str)?;
        let tx = match self.blockchain.get_tx(&txid)? {
            Some(t) => t,
            None => { warn!("License Tx {} not found", txid); return Ok(false); }
        };

        let target_script = Address::from_str(developer_addr)?.assume_checked().script_pubkey();
        let mut hasher = Sha256::new();
        hasher.update(format!("LICENSE{}", machine_id).as_bytes());
        let expected_hash = hasher.finalize();

        let mut paid_dev = false;
        let mut found_metadata = false;

        for output in tx.output {
            if output.script_pubkey == target_script && output.value >= required_sats {
                paid_dev = true;
            }
            if output.script_pubkey.is_op_return() {
                for instruction in output.script_pubkey.instructions() {
                    if let Ok(Instruction::PushBytes(data)) = instruction {
                        if data.as_bytes() == expected_hash.as_slice() {
                            found_metadata = true;
                        }
                    }
                }
            }
        }
        info!("License Audit {}: Payment={}, Metadata={}", txid, paid_dev, found_metadata);
        Ok(paid_dev && found_metadata)
    }
}
