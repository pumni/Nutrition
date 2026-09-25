//! Immediate-rejection, per-provider/model in-flight concurrency boundary.

use crate::{
    ModelIdentity, ProviderId, ProviderSelection, StructuredGenerationRequest,
    StructuredGenerationResponse, StructuredModel, StructuredModelError,
    StructuredModelErrorClassification,
};
use std::{fmt, sync::Arc};
use tokio::sync::Semaphore;

/// Conservative local default; deployments should set this after staging and quota review.
pub const DEFAULT_HOSTED_MAX_IN_FLIGHT: usize = 1;
/// Hard upper bound for a configured per-provider/model concurrency limit.
pub const MAXIMUM_HOSTED_MAX_IN_FLIGHT: usize = 64;

/// Invalid bulkhead capacity configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderModelBulkheadError {
    InvalidCapacity,
}

impl fmt::Display for ProviderModelBulkheadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider model bulkhead capacity is invalid")
    }
}

impl std::error::Error for ProviderModelBulkheadError {}

/// Bounds one resolved provider/model pair without queuing callers.
pub struct ProviderModelBulkhead {
    inner: Arc<dyn StructuredModel>,
    permits: Arc<Semaphore>,
    provider: ProviderId,
    model: ModelIdentity,
}

impl ProviderModelBulkhead {
    /// Wraps a validated registry selection with immediate-rejection concurrency control.
    ///
    /// # Errors
    ///
    /// Returns `InvalidCapacity` unless the limit is between one and the hard upper bound.
    pub fn new(
        selection: ProviderSelection,
        maximum_in_flight: usize,
    ) -> Result<Self, ProviderModelBulkheadError> {
        if !(1..=MAXIMUM_HOSTED_MAX_IN_FLIGHT).contains(&maximum_in_flight) {
            return Err(ProviderModelBulkheadError::InvalidCapacity);
        }
        let (provider, model, inner) = selection.into_parts();
        Ok(Self {
            inner,
            permits: Arc::new(Semaphore::new(maximum_in_flight)),
            provider,
            model,
        })
    }

    fn record_outcome(&self, outcome: &'static str) {
        metrics::counter!(
            "nutrition_structured_model_bulkhead_requests_total",
            "provider" => self.provider.as_str(),
            "model" => self.model.as_str().to_owned(),
            "outcome" => outcome
        )
        .increment(1);
    }
}

#[async_trait::async_trait]
impl StructuredModel for ProviderModelBulkhead {
    async fn generate(
        &self,
        request: &StructuredGenerationRequest,
        maximum_response_bytes: usize,
    ) -> Result<StructuredGenerationResponse, StructuredModelError> {
        let _permit = if let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() {
            self.record_outcome("accepted");
            permit
        } else {
            self.record_outcome("saturated");
            return Err(StructuredModelError::new(
                StructuredModelErrorClassification::CapacityRejected,
                "provider_bulkhead_saturated",
            ));
        };
        let _in_flight = InFlightGauge::new(self.provider, self.model.as_str());
        self.inner.generate(request, maximum_response_bytes).await
    }
}

struct InFlightGauge {
    provider: &'static str,
    model: String,
}

impl InFlightGauge {
    fn new(provider: ProviderId, model: &str) -> Self {
        metrics::gauge!(
            "nutrition_structured_model_bulkhead_in_flight",
            "provider" => provider.as_str(),
            "model" => model.to_owned()
        )
        .increment(1.0);
        Self {
            provider: provider.as_str(),
            model: model.to_owned(),
        }
    }
}

impl Drop for InFlightGauge {
    fn drop(&mut self) {
        metrics::gauge!(
            "nutrition_structured_model_bulkhead_in_flight",
            "provider" => self.provider,
            "model" => self.model.clone()
        )
        .decrement(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ModelIdentity, ProviderId, ProviderIdentity, ProviderSelection, StrictJsonSchema,
        StructuredGenerationRequest, StructuredModelErrorClassification,
        StructuredResponseMetadata, UntrustedInput,
    };
    use serde_json::json;
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::{Mutex, Notify};

    fn request() -> StructuredGenerationRequest {
        StructuredGenerationRequest {
            provider: ProviderIdentity::new("openai"),
            model: ModelIdentity::new("test-model-a"),
            system_instruction: "test".to_owned(),
            untrusted_input: UntrustedInput::new("private test request"),
            schema: StrictJsonSchema::new(json!({"type": "object"})),
        }
    }

    fn response() -> StructuredGenerationResponse {
        StructuredGenerationResponse {
            output: json!({}),
            metadata: StructuredResponseMetadata::default(),
        }
    }

    struct BlockingOnceModel {
        calls: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    impl BlockingOnceModel {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                started: Notify::new(),
                release: Notify::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl StructuredModel for BlockingOnceModel {
        async fn generate(
            &self,
            _request: &StructuredGenerationRequest,
            _maximum_response_bytes: usize,
        ) -> Result<StructuredGenerationResponse, StructuredModelError> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.started.notify_one();
                self.release.notified().await;
            }
            Ok(response())
        }
    }

    struct SequenceModel {
        responses: Mutex<VecDeque<Result<StructuredGenerationResponse, StructuredModelError>>>,
    }

    #[async_trait::async_trait]
    impl StructuredModel for SequenceModel {
        async fn generate(
            &self,
            _request: &StructuredGenerationRequest,
            _maximum_response_bytes: usize,
        ) -> Result<StructuredGenerationResponse, StructuredModelError> {
            self.responses
                .lock()
                .await
                .pop_front()
                .expect("test response")
        }
    }

    fn selection(model: &str, implementation: Arc<dyn StructuredModel>) -> ProviderSelection {
        ProviderSelection::for_test(ProviderId::OpenAi, model, implementation)
    }

    #[tokio::test]
    async fn saturation_is_immediate_and_isolated_by_model_identity() {
        let slow = Arc::new(BlockingOnceModel::new());
        let bulkhead_a = Arc::new(
            ProviderModelBulkhead::new(selection("test-model-a", slow.clone()), 1)
                .expect("valid capacity"),
        );
        let fast = Arc::new(SequenceModel {
            responses: Mutex::new(VecDeque::from([Ok(response())])),
        });
        let bulkhead_b =
            ProviderModelBulkhead::new(selection("test-model-b", fast), 1).expect("valid capacity");

        let first_bulkhead = Arc::clone(&bulkhead_a);
        let first_request = request();
        let first =
            tokio::spawn(async move { first_bulkhead.generate(&first_request, 1024).await });
        slow.started.notified().await;

        let saturation_request = request();
        let saturated = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            bulkhead_a.generate(&saturation_request, 1024),
        )
        .await
        .expect("the full model-specific bulkhead does not queue callers")
        .expect_err("the full model-specific bulkhead rejects immediately");
        assert_eq!(
            saturated.classification,
            StructuredModelErrorClassification::CapacityRejected
        );
        assert_eq!(saturated.code(), "provider_bulkhead_saturated");
        assert!(!saturated.code().contains("private test request"));
        assert_eq!(slow.calls.load(Ordering::SeqCst), 1);
        bulkhead_b
            .generate(&request(), 1024)
            .await
            .expect("a different model has its own capacity");

        slow.release.notify_one();
        first
            .await
            .expect("first provider task")
            .expect("first succeeds");
        bulkhead_a
            .generate(&request(), 1024)
            .await
            .expect("success releases the permit");
    }

    #[tokio::test]
    async fn provider_errors_release_the_permit() {
        let model = Arc::new(SequenceModel {
            responses: Mutex::new(VecDeque::from([
                Err(StructuredModelError::new(
                    StructuredModelErrorClassification::Permanent,
                    "test_provider_error",
                )),
                Ok(response()),
            ])),
        });
        let bulkhead = ProviderModelBulkhead::new(selection("test-model-a", model), 1)
            .expect("valid capacity");

        let error = bulkhead
            .generate(&request(), 1024)
            .await
            .expect_err("first response is a provider error");
        assert_eq!(error.code(), "test_provider_error");
        bulkhead
            .generate(&request(), 1024)
            .await
            .expect("provider error releases the permit");
    }

    #[test]
    fn capacity_is_bounded() {
        let model: Arc<dyn StructuredModel> = Arc::new(SequenceModel {
            responses: Mutex::new(VecDeque::new()),
        });
        assert!(matches!(
            ProviderModelBulkhead::new(selection("test-model-a", Arc::clone(&model)), 0),
            Err(ProviderModelBulkheadError::InvalidCapacity)
        ));
        assert!(matches!(
            ProviderModelBulkhead::new(
                selection("test-model-a", model),
                MAXIMUM_HOSTED_MAX_IN_FLIGHT + 1
            ),
            Err(ProviderModelBulkheadError::InvalidCapacity)
        ));
    }
}
