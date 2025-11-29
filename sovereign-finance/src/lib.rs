// crates/sovereign-finance/src/lib.rs

use bdk::bitcoin::{Address, Txid};
use bdk::bitcoin::blockdata::script::Instruction;
use bdk::blockchain::{ElectrumBlockchain, GetTx};
use bdk::electrum_client::Client;
use sha2::{Sha256, Digest};
use std::str::FromStr;
use log::{info, warn, error};
use anyhow::Context;

// We wrap the verifier in a struct that manages the connection.
// ElectrumBlockchain wraps an Arc<Client>, so it is cheap to clone and strictly Thread-Safe.
pub struct LicenseVerifier {
    blockchain: ElectrumBlockchain,
    // We add configuration for the developer address and required sats here
    // to encapsulate the "Business Logic" within the crate.
    developer_addr: String,
    required_sats: u64,
}

impl LicenseVerifier {
    pub fn new(electrum_url: &str, developer_addr: &str, required_sats: u64) -> anyhow::Result<Self> {
        // Validate inputs immediately to fail fast
        let _ = Address::from_str(developer_addr).context("Invalid Developer Address format")?;

        // Connect to Electrum. This is a blocking call, but it happens once at startup.
        let client = Client::new(electrum_url).context("Failed to connect to Electrum Server")?;

        Ok(Self {
            blockchain: ElectrumBlockchain::from(client),
            developer_addr: developer_addr.to_string(),
            required_sats,
        })
    }

    /// Verifies a machine-locked license on the Bitcoin blockchain.
    ///
    /// LOGIC:
    /// A valid license is a transaction that:
    /// 1. Pays >= required_sats to the developer address.
    /// 2. Contains an OP_RETURN output with SHA256("LICENSE" + machine_id).
    ///
    /// This function is BLOCKING. The caller must run it in a separate thread.
    pub fn verify_license_sync(&self, txid_str: &str, machine_id: &str) -> anyhow::Result<bool> {
        // 1. Type Conversion
        let txid = Txid::from_str(txid_str).context("Invalid TxID")?;
        let target_script = Address::from_str(&self.developer_addr)?.assume_checked().script_pubkey();

        // 2. Compute the "Binding Hash"
        // This cryptographically binds the license to THIS specific machine.
        // Even if the TxID is public, it cannot be reused on another machine
        // because the OP_RETURN hash wouldn't match the new machine's ID.
        let mut hasher = Sha256::new();
        hasher.update(format!("LICENSE{}", machine_id).as_bytes());
        let expected_hash = hasher.finalize();

        // 3. Network Query (The Blocking Step)
        let tx = match self.blockchain.get_tx(&txid) {
            Ok(Some(t)) => t,
            Ok(None) => {
                warn!("License Tx {} not found in blockchain history.", txid);
                return Ok(false);
            },
            Err(e) => {
                // We map network errors to anyhow::Error to avoid exposing electrum types
                error!("Electrum Network Error: {}", e);
                return Err(anyhow::anyhow!("Network error: {}", e));
            }
        };

        // 4. Verification Loop
        let mut paid_dev = false;
        let mut found_metadata = false;

        for output in tx.output {
            // Check Payment Condition
            if output.script_pubkey == target_script && output.value >= self.required_sats {
                paid_dev = true;
            }

            // Check Metadata Condition (OP_RETURN)
            if output.script_pubkey.is_op_return() {
                for instruction in output.script_pubkey.instructions() {
                    // We look for a PushBytes instruction containing our hash
                    if let Ok(Instruction::PushBytes(data)) = instruction {
                        if data.as_bytes() == expected_hash.as_slice() {
                            found_metadata = true;
                        }
                    }
                }
            }
        }

        info!("License Audit Result for {}: Payment={}, Metadata={}", txid, paid_dev, found_metadata);

        // Strict AND condition
        Ok(paid_dev && found_metadata)
    }
}

pub struct OldLicenseVerifier {
    blockchain: ElectrumBlockchain,
}

impl OldLicenseVerifier {
    pub fn new(electrum_url: &str) -> anyhow::Result<Self> {
        let client = Client::new(electrum_url)?;
        Ok(Self {
            blockchain: ElectrumBlockchain::from(client),
        })
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
            None => {
                warn!("License Tx {} not found", txid);
                return Ok(false);
            }
        };

        let target_script = Address::from_str(developer_addr)?
            .assume_checked()
            .script_pubkey();
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
        info!(
            "License Audit {}: Payment={}, Metadata={}",
            txid, paid_dev, found_metadata
        );
        Ok(paid_dev && found_metadata)
    }
}
