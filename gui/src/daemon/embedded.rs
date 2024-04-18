use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use super::{model::*, Daemon, DaemonError};
use async_trait::async_trait;
use liana::{
    commands::{CoinStatus, LabelItem},
    config::Config,
    miniscript::bitcoin::{address, psbt::Psbt, Address, OutPoint, Txid},
    DaemonControl, DaemonHandle,
};

pub struct EmbeddedDaemon {
    config: Config,
    handle: Mutex<Option<DaemonHandle>>,
}

impl EmbeddedDaemon {
    pub fn start(config: Config) -> Result<EmbeddedDaemon, DaemonError> {
        let handle = DaemonHandle::start_default(config.clone()).map_err(DaemonError::Start)?;
        Ok(Self {
            handle: Mutex::new(Some(handle)),
            config,
        })
    }

    pub fn command<T, F>(&self, method: F) -> Result<T, DaemonError>
    where
        F: FnOnce(&DaemonControl) -> Result<T, DaemonError>,
    {
        match self.handle.lock()?.as_ref() {
            Some(DaemonHandle::Controller { control, .. }) => method(control),
            None => Err(DaemonError::DaemonStopped),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for DaemonError {
    fn from(value: std::sync::PoisonError<T>) -> Self {
        DaemonError::Unexpected(format!("Daemon panic: {}", value))
    }
}

impl std::fmt::Debug for EmbeddedDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonHandle").finish()
    }
}

#[async_trait]
impl Daemon for EmbeddedDaemon {
    fn is_external(&self) -> bool {
        false
    }

    fn config(&self) -> Option<&Config> {
        Some(&self.config)
    }

    async fn is_alive(&self) -> Result<(), DaemonError> {
        let mut handle = self.handle.lock()?;
        if let Some(h) = handle.as_ref() {
            if h.is_alive() {
                return Ok(());
            }
        }
        // if the daemon poller is not alive, we try to terminate it to fetch the error.
        if let Some(h) = handle.take() {
            h.stop()
                .map_err(|e| DaemonError::Unexpected(e.to_string()))?;
        }
        Ok(())
    }

    async fn stop(&self) -> Result<(), DaemonError> {
        let mut handle = self.handle.lock()?;
        if let Some(h) = handle.take() {
            h.stop()
                .map_err(|e| DaemonError::Unexpected(e.to_string()))?;
        }
        Ok(())
    }

    async fn get_info(&self) -> Result<GetInfoResult, DaemonError> {
        self.command(|daemon| Ok(daemon.get_info()))
    }

    async fn get_new_address(&self) -> Result<GetAddressResult, DaemonError> {
        self.command(|daemon| Ok(daemon.get_new_address()))
    }

    async fn list_coins(
        &self,
        statuses: &[CoinStatus],
        outpoints: &[OutPoint],
    ) -> Result<ListCoinsResult, DaemonError> {
        self.command(|daemon| Ok(daemon.list_coins(statuses, outpoints)))
    }

    async fn list_spend_txs(&self) -> Result<ListSpendResult, DaemonError> {
        self.command(|daemon| {
            daemon
                .list_spend(None)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn list_confirmed_txs(
        &self,
        start: u32,
        end: u32,
        limit: u64,
    ) -> Result<ListTransactionsResult, DaemonError> {
        self.command(|daemon| Ok(daemon.list_confirmed_transactions(start, end, limit)))
    }

    async fn list_txs(&self, txids: &[Txid]) -> Result<ListTransactionsResult, DaemonError> {
        self.command(|daemon| Ok(daemon.list_transactions(txids)))
    }

    async fn create_spend_tx(
        &self,
        coins_outpoints: &[OutPoint],
        destinations: &HashMap<Address<address::NetworkUnchecked>, u64>,
        feerate_vb: u64,
        change_address: Option<Address<address::NetworkUnchecked>>,
    ) -> Result<CreateSpendResult, DaemonError> {
        self.command(|daemon| {
            daemon
                .create_spend(destinations, coins_outpoints, feerate_vb, change_address)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn rbf_psbt(
        &self,
        txid: &Txid,
        is_cancel: bool,
        feerate_vb: Option<u64>,
    ) -> Result<CreateSpendResult, DaemonError> {
        self.command(|daemon| {
            daemon
                .rbf_psbt(txid, is_cancel, feerate_vb)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn update_spend_tx(&self, psbt: &Psbt) -> Result<(), DaemonError> {
        self.command(|daemon| {
            daemon
                .update_spend(psbt.clone())
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn delete_spend_tx(&self, txid: &Txid) -> Result<(), DaemonError> {
        self.command(|daemon| {
            daemon.delete_spend(txid);
            Ok(())
        })
    }

    async fn broadcast_spend_tx(&self, txid: &Txid) -> Result<(), DaemonError> {
        self.command(|daemon| {
            daemon
                .broadcast_spend(txid)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn start_rescan(&self, t: u32) -> Result<(), DaemonError> {
        self.command(|daemon| {
            daemon
                .start_rescan(t)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn create_recovery(
        &self,
        address: Address<address::NetworkUnchecked>,
        feerate_vb: u64,
        sequence: Option<u16>,
    ) -> Result<Psbt, DaemonError> {
        self.command(|daemon| {
            daemon
                .create_recovery(address, feerate_vb, sequence)
                .map(|res| res.psbt)
                .map_err(|e| DaemonError::Unexpected(e.to_string()))
        })
    }

    async fn get_labels(
        &self,
        items: &HashSet<LabelItem>,
    ) -> Result<HashMap<String, String>, DaemonError> {
        self.command(|daemon| Ok(daemon.get_labels(items).labels))
    }

    async fn update_labels(
        &self,
        items: &HashMap<LabelItem, Option<String>>,
    ) -> Result<(), DaemonError> {
        self.command(|daemon| {
            daemon.update_labels(items);
            Ok(())
        })
    }
}
