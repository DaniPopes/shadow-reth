use std::str::FromStr;

use jsonrpsee::{
    core::{async_trait, RpcResult},
    types::{error::INTERNAL_ERROR_CODE, ErrorObject},
};
use reth::providers::{BlockNumReader, BlockReaderIdExt};
use reth_primitives::{revm_primitives::FixedBytes, BlockNumberOrTag};
use serde::{Deserialize, Serialize};

use crate::{ShadowRpc, ShadowRpcApiServer};

/// `shadow_getLogs` RPC request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLogsRpcRequest {
    /// Request identifier established by a client.
    pub id: String,
    /// Specifies version of JSON-RPC protocol.
    pub json_rpc: String,
    /// Indicates the method to be invoked.
    pub method: String,
    /// Contains parameters for request.
    pub params: Vec<GetLogsParameters>,
}

/// Unvalidated parameters for `shadow_getLogs` RPC requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetLogsParameters {
    /// Contains contract addresses from which logs should originate.
    pub address: Vec<String>,
    /// Hash of block from which logs should originate. Using this field is equivalent
    /// to passing identical values for `fromBlock` and `toBlock`.
    pub block_hash: Option<String>,
    /// Start of block range from which logs should originate.
    pub from_block: Option<String>,
    /// End of block range from which logs should originate.
    pub to_block: Option<String>,
    /// Array of 32-byte data topics.
    pub topics: Vec<String>,
}

/// `shadow_getLogs` RPC response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetLogsResponse {
    /// Identifier for response.
    pub id: String,
    /// Specifies version of JSON-RPC protocol.
    pub json_rpc: String,
    /// Contains result sets from successful request execution.
    pub result: Vec<GetLogsResult>,
}

/// Inner result type for `shadow_getLogs` RPC responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetLogsResult {
    /// Contract address from which the log originated.
    pub address: String,
    /// Hash of block from which the log originated.
    pub block_hash: String,
    /// Block number from which the log originated.
    pub block_number: String,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub data: String,
    /// Integer of the log index in the containing block.
    pub log_index: String,
    /// Indicates whether the log has been removed from the canonical chain.
    pub removed: bool,
    /// Array of topics.
    pub topics: [Option<String>; 4],
    /// Hash of transaction from which the log originated.
    pub transaction_hash: String,
    /// Integer of the transaction index position from which the log originated.
    pub transaction_index: String,
}
/// Helper type for ease of use in converting rows from the `shadow_getLogs`
/// query into the `GetLogsResult` type which is used in `GetLogsResponse`.
#[derive(Debug, sqlx::FromRow)]
pub struct RawGetLogsRow {
    /// Address from which a log originated.
    pub address: Vec<u8>,
    /// Hash of bock from which a log orignated.
    pub block_hash: Vec<u8>,
    /// Integer of the log index position in its containing block.
    pub block_log_index: i64,
    /// Block number from which a log originated.
    pub block_number: i64,
    /// Contains one or more 32-byte non-indexed arguments of the log.
    pub data: Vec<u8>,
    /// Indicates whether a log was removed from the canonical chain.
    pub removed: bool,
    /// Hash of event signature.
    pub topic_0: Option<Vec<u8>>,
    /// Additional topic #1.
    pub topic_1: Option<Vec<u8>>,
    /// Additional topic #2.
    pub topic_2: Option<Vec<u8>>,
    /// Additional topic #3.
    pub topic_3: Option<Vec<u8>>,
    /// Hash of the transaction from which a log originated.
    pub transaction_hash: Vec<u8>,
    /// Integer of the transaction index position in a log's containing block.
    pub transaction_index: i64,
    /// Integer of the log index position within a transaction.
    pub transaction_log_index: i64,
}

/// Validated query parameter object. Instances are considered to be well-formed
/// and are used in query construction and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedQueryParams {
    /// Start of block range from which logs will be filtered.
    pub(crate) from_block: u64,
    /// End of block range from which logs will be filtered.
    pub(crate) to_block: u64,
    /// Set of addresses from which logs will be filtered.
    pub(crate) addresses: Vec<String>,
    /// Set of log topics.
    pub(crate) topics: [Option<String>; 4],
}

impl From<RawGetLogsRow> for GetLogsResult {
    fn from(value: RawGetLogsRow) -> Self {
        Self {
            address: format!("0x{}", hex::encode(value.address)),
            block_hash: format!("0x{}", hex::encode(value.block_hash)),
            block_number: hex::encode(value.block_number.to_be_bytes()),
            data: format!("0x{}", hex::encode(value.data)),
            log_index: value.block_log_index.to_string(),
            removed: value.removed,
            topics: [
                value.topic_0.map(|t| format!("0x{}", hex::encode(t))),
                value.topic_1.map(|t| format!("0x{}", hex::encode(t))),
                value.topic_2.map(|t| format!("0x{}", hex::encode(t))),
                value.topic_3.map(|t| format!("0x{}", hex::encode(t))),
            ],
            transaction_hash: format!("0x{}", hex::encode(value.transaction_hash)),
            transaction_index: value.transaction_index.to_string(),
        }
    }
}

#[async_trait]
impl<P> ShadowRpcApiServer for ShadowRpc<P>
where
    P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static,
{
    #[doc = "Returns shadow logs."]
    async fn get_logs(&self, req: GetLogsRpcRequest) -> RpcResult<GetLogsResponse> {
        let base_stmt = r#"
            SELECT
                address,
                block_hash,
                block_log_index,
                block_number,
                data,
                removed,
                topic_0,
                topic_1,
                topic_2,
                topic_3,
                transaction_hash,
                transaction_index,
                transaction_log_index
            FROM shadow_logs
        "#;

        let validated_param_objs = req
            .params
            .into_iter()
            .map(|param_obj| ValidatedQueryParams::new(&self.provider, param_obj))
            .collect::<RpcResult<Vec<ValidatedQueryParams>>>()?;

        let mut results: Vec<GetLogsResult> = vec![];
        for query_params in validated_param_objs {
            let sql = format!("{base_stmt} {query_params}");
            let raw_rows: Vec<RawGetLogsRow> = sqlx::query_as(&sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| ErrorObject::owned::<()>(INTERNAL_ERROR_CODE, e.to_string(), None))?;
            let mut result =
                raw_rows.into_iter().map(GetLogsResult::from).collect::<Vec<GetLogsResult>>();
            results.append(&mut result);
        }

        Ok(GetLogsResponse { id: req.id, json_rpc: req.json_rpc, result: results })
    }
}

impl ValidatedQueryParams {
    fn new(
        provider: &(impl BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static),
        params: GetLogsParameters,
    ) -> RpcResult<Self> {
        let (from_block, to_block) = match (params.block_hash, params.from_block, params.to_block) {
            (None, None, None) => {
                let num = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (num, num)
            }
            (None, None, Some(to_block)) => {
                let from = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                let to_tag = BlockNumberOrTag::from_str(&to_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let to = match provider.block_by_number_or_tag(to_tag) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block number or tag: {to_tag}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (from, to)
            }
            (None, Some(from_block), None) => {
                let from_tag = BlockNumberOrTag::from_str(&from_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let from = match provider.block_by_number_or_tag(from_tag) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block number or tag: {from_tag}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                let to = match provider.block_by_number_or_tag(BlockNumberOrTag::Latest) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            "No block found for block number or tag: latest",
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (from, to)
            }
            (None, Some(from_block), Some(to_block)) => {
                let from_tag = BlockNumberOrTag::from_str(&from_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let from = match provider.block_by_number_or_tag(from_tag) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block number or tag: {from_tag}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                let to_tag = BlockNumberOrTag::from_str(&to_block)
                    .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?;
                let to = match provider.block_by_number_or_tag(to_tag) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block number or tag: {to_tag}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };

                (from, to)
            }
            (Some(block_hash), None, None) => {
                let num = match provider.block_by_hash(
                    FixedBytes::from_str(&block_hash)
                        .map_err(|e| ErrorObject::owned::<()>(-1, e.to_string(), None))?,
                ) {
                    Ok(Some(b)) => b.number,
                    Ok(None) => {
                        return Err(ErrorObject::owned::<()>(
                            -1,
                            format!("No block found for block hash: {block_hash}"),
                            None,
                        ))
                    }
                    Err(e) => return Err(ErrorObject::owned::<()>(-1, e.to_string(), None)),
                };
                (num, num)
            }
            (Some(_), Some(_), _) | (Some(_), _, Some(_)) => return Err(ErrorObject::owned::<()>(
                -32001,
                "Parameters fromBlock and toBlock cannot be used if blockHash parameter is present",
                None,
            )),
        };

        if params.topics.len() > 4 {
            return Err(ErrorObject::owned::<()>(32002, "Only up to four topics are allowed", None));
        }

        let mut topics: [Option<String>; 4] = [None, None, None, None];

        for (idx, topic) in params.topics.into_iter().enumerate() {
            topics[idx] = Some(topic);
        }

        Ok(ValidatedQueryParams { from_block, to_block, addresses: params.address, topics })
    }
}

impl std::fmt::Display for ValidatedQueryParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let address_clause = if !self.addresses.is_empty() {
            Some(format!(
                "address IN ({})",
                self.addresses
                    .iter()
                    .map(|addr| format!("X'{}'", &addr[2..]))
                    .collect::<Vec<String>>()
                    .join(", ")
            ))
        } else {
            None
        };

        let block_range_clause =
            Some(format!("block_number BETWEEN {} AND {}", self.from_block, self.to_block));

        let topic_0_clause = self.topics[0].as_ref().map(|t0| format!("topic_0 = {t0}"));

        let topic_1_clause = self.topics[1].as_ref().map(|t1| format!("topic_1 = {t1}"));

        let topic_2_clause = self.topics[2].as_ref().map(|t2| format!("topic_2 = {t2}"));

        let topic_3_clause = self.topics[3].as_ref().map(|t3| format!("topic_3 = {t3}"));

        let clauses = [
            address_clause,
            block_range_clause,
            topic_0_clause,
            topic_1_clause,
            topic_2_clause,
            topic_3_clause,
        ];

        let filtered_clauses = clauses.into_iter().flatten().collect::<Vec<String>>();

        if !filtered_clauses.is_empty() {
            write!(f, "WHERE {}", filtered_clauses.join(" AND "))
        } else {
            write!(f, "")
        }
    }
}

#[cfg(test)]
mod tests {
    use reth::providers::test_utils::MockEthProvider;
    use reth_primitives::{Block, Header};

    use super::ValidatedQueryParams;

    use super::GetLogsParameters;

    #[tokio::test]
    async fn test_query_param_validation() {
        let mock_provider = MockEthProvider::default();

        let first_block =
            Block { header: Header { number: 0, ..Default::default() }, ..Default::default() };
        let first_block_hash = first_block.hash_slow();

        let last_block =
            Block { header: Header { number: 10, ..Default::default() }, ..Default::default() };
        let last_block_hash = last_block.hash_slow();

        mock_provider
            .extend_blocks([(first_block_hash, first_block), (last_block_hash, last_block)]);

        let params_with_block_hash = GetLogsParameters {
            address: vec!["0x123".to_string()],
            block_hash: Some(last_block_hash.to_string()),
            from_block: None,
            to_block: None,
            topics: vec![],
        };

        assert!(ValidatedQueryParams::new(&mock_provider, params_with_block_hash).is_ok());

        let params_with_defaults = GetLogsParameters {
            address: vec!["0x123".to_string()],
            block_hash: None,
            from_block: None,
            to_block: None,
            topics: vec![],
        };

        let validated = ValidatedQueryParams::new(&mock_provider, params_with_defaults);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x123".to_string()],
                from_block: 10,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_block_tags = GetLogsParameters {
            address: vec!["0x123".to_string()],
            block_hash: None,
            from_block: Some("earliest".to_string()),
            to_block: Some("latest".to_string()),
            topics: vec![],
        };
        let validated = ValidatedQueryParams::new(&mock_provider, params_with_block_tags);

        assert_eq!(
            validated.unwrap(),
            ValidatedQueryParams {
                addresses: vec!["0x123".to_string()],
                from_block: 0,
                to_block: 10,
                topics: [None, None, None, None]
            }
        );

        let params_with_block_hash_and_range = GetLogsParameters {
            address: vec!["0x123".to_string()],
            block_hash: Some(first_block_hash.to_string()),
            from_block: Some(first_block_hash.to_string()),
            to_block: Some(last_block_hash.to_string()),
            topics: vec![],
        };
        assert!(
            ValidatedQueryParams::new(&mock_provider, params_with_block_hash_and_range).is_err()
        );
    }
}