// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    failpoint::fail_point,
    metrics::metrics,
    page::Page,
    param::{AddressParam, TransactionIdParam},
};

use aptos_api_types::{
    mime_types::BCS_SIGNED_TRANSACTION, AsConverter, Error, LedgerInfo, Response, Transaction,
    TransactionData, TransactionId, TransactionOnChainData, TransactionSigningMessage,
    UserCreateSigningMessageRequest, UserTransactionRequest,
};
use aptos_crypto::signing_message;
use aptos_types::{
    mempool_status::MempoolStatusCode,
    transaction::{RawTransaction, RawTransactionWithData, SignedTransaction},
};

use anyhow::Result;
use warp::{
    filters::BoxedFilter,
    http::{header::CONTENT_TYPE, StatusCode},
    reply, Filter, Rejection, Reply,
};

// GET /transactions/{txn-hash / version}
pub fn get_transaction(context: Context) -> BoxedFilter<(impl Reply,)> {
    warp::path!("transactions" / TransactionIdParam)
        .and(warp::get())
        .and(context.filter())
        .and_then(handle_get_transaction)
        .with(metrics("get_transaction"))
        .boxed()
}

// GET /transactions?start={u64}&limit={u16}
pub fn get_transactions(context: Context) -> BoxedFilter<(impl Reply,)> {
    warp::path!("transactions")
        .and(warp::get())
        .and(warp::query::<Page>())
        .and(context.filter())
        .and_then(handle_get_transactions)
        .with(metrics("get_transactions"))
        .boxed()
}

// GET /accounts/{address}/transactions?start={u64}&limit={u16}
pub fn get_account_transactions(context: Context) -> BoxedFilter<(impl Reply,)> {
    warp::path!("accounts" / AddressParam / "transactions")
        .and(warp::get())
        .and(warp::query::<Page>())
        .and(context.filter())
        .and_then(handle_get_account_transactions)
        .with(metrics("get_account_transactions"))
        .boxed()
}

// POST /transactions with JSON
pub fn submit_json_transactions(context: Context) -> BoxedFilter<(impl Reply,)> {
    warp::path!("transactions")
        .and(warp::post())
        .and(warp::body::content_length_limit(
            context.content_length_limit(),
        ))
        .and(warp::body::json::<UserTransactionRequest>())
        .and(context.filter())
        .and_then(handle_submit_json_transactions)
        .with(metrics("submit_json_transactions"))
        .boxed()
}

// POST /transactions with BCS
pub fn submit_bcs_transactions(context: Context) -> BoxedFilter<(impl Reply,)> {
    // The `warp::body::bytes` does not check content-type like `warp::body::json`,
    // so we used `warp::header::exact` to ensure only BCS signed txn matches this route.
    // When the content-type is invalid (not json / bcs signed txn), `submit_json_transactions`
    // route will emit correct rejection (UnsupportedMediaType) which will be handled by recover
    // handler, the invalid header error should be ignored.
    warp::path!("transactions")
        .and(warp::post())
        .and(warp::body::content_length_limit(
            context.content_length_limit(),
        ))
        .and(warp::header::exact(
            CONTENT_TYPE.as_str(),
            BCS_SIGNED_TRANSACTION,
        ))
        .and(warp::body::bytes())
        .and(context.filter())
        .and_then(handle_submit_bcs_transactions)
        .with(metrics("submit_bcs_transactions"))
        .boxed()
}

// POST /transactions/signing_message
pub fn create_signing_message(context: Context) -> BoxedFilter<(impl Reply,)> {
    warp::path!("transactions" / "signing_message")
        .and(warp::post())
        .and(warp::body::content_length_limit(
            context.content_length_limit(),
        ))
        .and(warp::body::json::<UserCreateSigningMessageRequest>())
        .and(context.filter())
        .and_then(handle_create_signing_message)
        .with(metrics("create_signing_message"))
        .boxed()
}

async fn handle_get_transaction(
    id: TransactionIdParam,
    context: Context,
) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_get_transaction")?;
    Ok(Transactions::new(context)?
        .get_transaction(id.parse("transaction hash or version")?)
        .await?)
}

async fn handle_get_transactions(page: Page, context: Context) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_get_transactions")?;
    Ok(Transactions::new(context)?.list(page)?)
}

async fn handle_get_account_transactions(
    address: AddressParam,
    page: Page,
    context: Context,
) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_get_account_transactions")?;
    Ok(Transactions::new(context)?.list_by_account(address, page)?)
}

async fn handle_submit_json_transactions(
    body: UserTransactionRequest,
    context: Context,
) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_submit_json_transactions")?;
    Ok(Transactions::new(context)?
        .create_from_request(body)
        .await?)
}

async fn handle_submit_bcs_transactions(
    body: bytes::Bytes,
    context: Context,
) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_submit_bcs_transactions")?;
    let txn = bcs::from_bytes(&body)
        .map_err(|err| Error::invalid_request_body(format!("deserialize error: {}", err)))?;
    Ok(Transactions::new(context)?.create(txn).await?)
}

async fn handle_create_signing_message(
    body: UserCreateSigningMessageRequest,
    context: Context,
) -> Result<impl Reply, Rejection> {
    fail_point("endpoint_create_signing_message")?;
    Ok(Transactions::new(context)?.signing_message(body)?)
}

struct Transactions {
    ledger_info: LedgerInfo,
    context: Context,
}

impl Transactions {
    fn new(context: Context) -> Result<Self, Error> {
        let ledger_info = context.get_latest_ledger_info()?;
        Ok(Self {
            ledger_info,
            context,
        })
    }

    pub async fn create_from_request(
        self,
        req: UserTransactionRequest,
    ) -> Result<impl Reply, Error> {
        let txn = self
            .context
            .move_resolver()?
            .as_converter()
            .try_into_signed_transaction(req, self.context.chain_id())
            .map_err(|e| {
                Error::invalid_request_body(format!(
                    "failed to create SignedTransaction from UserTransactionRequest: {}",
                    e
                ))
            })?;
        self.create(txn).await
    }

    pub async fn create(self, txn: SignedTransaction) -> Result<impl Reply, Error> {
        let (mempool_status, vm_status_opt) = self.context.submit_transaction(txn.clone()).await?;
        match mempool_status.code {
            MempoolStatusCode::Accepted => {
                let resolver = self.context.move_resolver()?;
                let pending_txn = resolver.as_converter().try_into_pending_transaction(txn)?;
                let resp = Response::new(self.ledger_info, &pending_txn)?;
                Ok(reply::with_status(resp, StatusCode::ACCEPTED))
            }
            MempoolStatusCode::VmError => Err(Error::bad_request(format!(
                "invalid transaction: {}",
                vm_status_opt
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "UNKNOWN".to_owned())
            ))),
            _ => Err(Error::bad_request(format!(
                "transaction is rejected: {}",
                mempool_status,
            ))),
        }
    }

    pub fn list(self, page: Page) -> Result<impl Reply, Error> {
        let ledger_version = self.ledger_info.version();
        let limit = page.limit()?;
        let last_page_start = if ledger_version > (limit as u64) {
            ledger_version - (limit as u64)
        } else {
            0
        };
        let start_version = page.start(last_page_start, ledger_version)?;

        let data = self
            .context
            .get_transactions(start_version, limit, ledger_version)?;

        self.render_transactions(data)
    }

    pub fn list_by_account(self, address: AddressParam, page: Page) -> Result<impl Reply, Error> {
        let data = self.context.get_account_transactions(
            address.parse("account address")?.into(),
            page.start(0, u64::MAX)?,
            page.limit()?,
            self.ledger_info.version(),
        )?;
        self.render_transactions(data)
    }

    fn render_transactions(self, data: Vec<TransactionOnChainData>) -> Result<impl Reply, Error> {
        if data.is_empty() {
            let txns: Vec<Transaction> = vec![];
            return Response::new(self.ledger_info, &txns);
        }
        let first_version = data[0].version;
        let mut timestamp = self.context.get_block_timestamp(first_version)?;
        let resolver = self.context.move_resolver()?;
        let converter = resolver.as_converter();
        let txns: Vec<Transaction> = data
            .into_iter()
            .map(|t| {
                let txn = converter.try_into_onchain_transaction(timestamp, t)?;
                // update timestamp, when txn is metadata block transaction
                // new timestamp is used for the following transactions
                timestamp = txn.timestamp();
                Ok(txn)
            })
            .collect::<Result<_>>()?;
        Response::new(self.ledger_info, &txns)
    }

    pub async fn get_transaction(self, id: TransactionId) -> Result<impl Reply, Error> {
        let txn_data = match id.clone() {
            TransactionId::Hash(hash) => self.get_by_hash(hash.into()).await?,
            TransactionId::Version(version) => self.get_by_version(version)?,
        }
        .ok_or_else(|| self.transaction_not_found(id))?;

        let resolver = self.context.move_resolver()?;
        let txn = match txn_data {
            TransactionData::OnChain(txn) => {
                let timestamp = self.context.get_block_timestamp(txn.version)?;
                resolver
                    .as_converter()
                    .try_into_onchain_transaction(timestamp, txn)?
            }
            TransactionData::Pending(txn) => {
                resolver.as_converter().try_into_pending_transaction(*txn)?
            }
        };

        Response::new(self.ledger_info, &txn)
    }

    pub fn signing_message(
        self,
        UserCreateSigningMessageRequest {
            transaction,
            secondary_signers,
        }: UserCreateSigningMessageRequest,
    ) -> Result<impl Reply, Error> {
        let resolver = self.context.move_resolver()?;
        let raw_txn: RawTransaction = resolver
            .as_converter()
            .try_into_raw_transaction(transaction, self.context.chain_id())
            .map_err(|e| {
                Error::invalid_request_body(format!("invalid UserTransactionRequest: {:?}", e))
            })?;

        let raw_message = match secondary_signers {
            Some(secondary_signer_addresses) => {
                signing_message(&RawTransactionWithData::new_multi_agent(
                    raw_txn,
                    secondary_signer_addresses
                        .into_iter()
                        .map(|v| v.into())
                        .collect(),
                ))
            }
            None => raw_txn.signing_message(),
        };

        Response::new(
            self.ledger_info,
            &TransactionSigningMessage::new(raw_message),
        )
    }

    fn transaction_not_found(&self, id: TransactionId) -> Error {
        Error::not_found("transaction", id, self.ledger_info.version())
    }

    fn get_by_version(&self, version: u64) -> Result<Option<TransactionData>> {
        if version > self.ledger_info.version() {
            return Ok(None);
        }
        Ok(Some(
            self.context
                .get_transaction_by_version(version, self.ledger_info.version())?
                .into(),
        ))
    }

    // This function looks for the transaction by hash in database and then mempool,
    // because the period a transaction stay in the mempool is likely short.
    // Although the mempool get transation is async, but looking up txn in database is a sync call,
    // thus we keep it simple and call them in sequence.
    async fn get_by_hash(&self, hash: aptos_crypto::HashValue) -> Result<Option<TransactionData>> {
        let from_db = self
            .context
            .get_transaction_by_hash(hash, self.ledger_info.version())?;
        Ok(match from_db {
            None => self
                .context
                .get_pending_transaction_by_hash(hash)
                .await?
                .map(|t| t.into()),
            _ => from_db.map(|t| t.into()),
        })
    }
}
