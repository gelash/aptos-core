// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::service::TelemetryEvent;
use aptos_config::config::NodeConfig;
use state_sync_driver::metrics::StorageSynchronizerOperations;
use std::collections::BTreeMap;

/// Core metrics event name
const APTOS_NODE_CORE_METRICS: &str = "APTOS_NODE_CORE_METRICS";

/// Core metric keys
const CONSENSUS_LAST_COMMITTED_ROUND: &str = "consensus_last_committed_round";
const CONSENSUS_PROPOSALS_COUNT: &str = "consensus_proposals_count";
const CONSENSUS_TIMEOUT_COUNT: &str = "consensus_timeout_count";
const MEMPOOL_CORE_MEMPOOL_INDEX_SIZE: &str = "mempool_core_mempool_index_size";
const STATE_SYNC_SYNCED_VERSION: &str = "state_sync_synced_version";
const STATE_SYNC_SYNCED_EPOCH: &str = "state_sync_synced_epoch";
const STORAGE_LEDGER_VERSION: &str = "storage_ledger_version";
const STORAGE_MIN_READABLE_LEDGER_VERSION: &str = "storage_min_readable_ledger_version";
const STORAGE_MIN_READABLE_STATE_VERSION: &str = "storage_min_readable_state_version";
const ROLE_TYPE: &str = "role_type";

// TODO(joshlind): add metrics for REST API and telemetry

/// Collects and sends the build information via telemetry
pub(crate) async fn create_core_metric_telemetry_event(node_config: &NodeConfig) -> TelemetryEvent {
    // Collect the core metrics
    let core_metrics = get_core_metrics(node_config);

    // Create and return a new telemetry event
    TelemetryEvent {
        name: APTOS_NODE_CORE_METRICS.into(),
        params: core_metrics,
    }
}

/// Used to expose core metrics for the node
pub fn get_core_metrics(node_config: &NodeConfig) -> BTreeMap<String, String> {
    let mut core_metrics: BTreeMap<String, String> = BTreeMap::new();
    collect_core_metrics(&mut core_metrics, node_config);
    core_metrics
}

/// Collects the core metrics and appends them to the given map
fn collect_core_metrics(core_metrics: &mut BTreeMap<String, String>, node_config: &NodeConfig) {
    // Collect the core metrics for each component
    collect_consensus_metrics(core_metrics);
    collect_mempool_metrics(core_metrics);
    collect_state_sync_metrics(core_metrics, node_config);
    collect_storage_metrics(core_metrics);

    // Collect the node role
    let node_role_type = node_config.base.role;
    core_metrics.insert(ROLE_TYPE.into(), node_role_type.as_str().into());
}

/// Collects the consensus metrics and appends it to the given map
fn collect_consensus_metrics(core_metrics: &mut BTreeMap<String, String>) {
    core_metrics.insert(
        CONSENSUS_PROPOSALS_COUNT.into(),
        consensus::counters::PROPOSALS_COUNT.get().to_string(),
    );
    core_metrics.insert(
        CONSENSUS_LAST_COMMITTED_ROUND.into(),
        consensus::counters::LAST_COMMITTED_ROUND.get().to_string(),
    );
    core_metrics.insert(
        CONSENSUS_TIMEOUT_COUNT.into(),
        consensus::counters::TIMEOUT_COUNT.get().to_string(),
    );
    //TODO(joshlind): add block tracing and back pressure!
}

/// Collects the mempool metrics and appends it to the given map
fn collect_mempool_metrics(core_metrics: &mut BTreeMap<String, String>) {
    core_metrics.insert(
        MEMPOOL_CORE_MEMPOOL_INDEX_SIZE.into(),
        aptos_mempool::counters::CORE_MEMPOOL_INDEX_SIZE
            .with_label_values(&["system_ttl"])
            .get()
            .to_string(),
    );
}

/// Collects the state sync metrics and appends it to the given map
fn collect_state_sync_metrics(
    core_metrics: &mut BTreeMap<String, String>,
    node_config: &NodeConfig,
) {
    // Depending on which state sync version is running, we need to grab the
    // appropriate counter. Otherwise, the node will panic due to a previously
    // registered lazy.
    // TODO(joshlind): remove this when v1 is gone!
    if node_config
        .state_sync
        .state_sync_driver
        .enable_state_sync_v2
    {
        core_metrics.insert(
            STATE_SYNC_SYNCED_EPOCH.into(),
            state_sync_driver::metrics::STORAGE_SYNCHRONIZER_OPERATIONS
                .with_label_values(&[StorageSynchronizerOperations::SyncedEpoch.get_label()])
                .get()
                .to_string(),
        );
        core_metrics.insert(
            STATE_SYNC_SYNCED_VERSION.into(),
            state_sync_driver::metrics::STORAGE_SYNCHRONIZER_OPERATIONS
                .with_label_values(&[StorageSynchronizerOperations::Synced.get_label()])
                .get()
                .to_string(),
        );
    } else {
        core_metrics.insert(
            STATE_SYNC_SYNCED_EPOCH.into(),
            state_sync_v1::counters::VERSION
                .with_label_values(&["synced"])
                .get()
                .to_string(),
        );
    }
    // TODO(joshlind): populate the state sync mode using the config!
}

/// Collects the storage metrics and appends it to the given map
fn collect_storage_metrics(core_metrics: &mut BTreeMap<String, String>) {
    core_metrics.insert(
        STORAGE_LEDGER_VERSION.into(),
        aptosdb::metrics::LEDGER_VERSION.get().to_string(),
    );
    core_metrics.insert(
        STORAGE_MIN_READABLE_LEDGER_VERSION.into(),
        aptosdb::metrics::PRUNER_LEAST_READABLE_VERSION
            .with_label_values(&["ledger_pruner"])
            .get()
            .to_string(),
    );
    core_metrics.insert(
        STORAGE_MIN_READABLE_STATE_VERSION.into(),
        aptosdb::metrics::PRUNER_LEAST_READABLE_VERSION
            .with_label_values(&["state_store"])
            .get()
            .to_string(),
    );
    // TODO(joshlind): add storage latencies!
}
