use chrono::{DateTime, Utc};
use tracing_subscriber::registry::LookupSpan;

// TODO: review pub vs. pub(crate)
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceCtx {
    pub parent_span: Option<SpanId>,
    pub trace_id: TraceId,
}

// todo extend error and etc
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum TraceCtxError {
    TelemetryLayerNotRegistered,
    RegistrySubscriberNotRegistered,
    NoEnabledSpan,
    NoParentNodeHasTraceCtx, // no parent node has explicitly registered trace ctx
}

// TODO: check in with rain & etc re: names, ideally find better options
impl TraceCtx {
    /// attempts to parse a TraceCtx from a string
    pub fn from_string(s: &str) -> Option<Self> {
        let mut iter = s.split(':');
        let s1 = iter.next()?;
        let trace_id = TraceId::from_string(s1)?;

        let s2 = iter.next()?;
        let parent_span = if s2 == "root" {
            None
        } else {
            let span_id = SpanId::from_string(s2)?;
            Some(span_id)
        };

        Some(TraceCtx {
            parent_span,
            trace_id,
        })
    }

    /// convert a TraceCtx to a string that can be included in headers, RPC framework metadata fields, etc
    pub fn to_string(&self) -> String {
        format!(
            "{}:{}",
            self.trace_id.to_string(),
            self.parent_span
                .as_ref()
                .map_or_else(|| "root".to_string(), |s| s.to_string())
        )
    }

    /// Generate a new 'TraceCtx' with a random trace id and no parent span
    pub fn new_root() -> Self {
        TraceCtx {
            trace_id: TraceId::generate(),
            parent_span: None,
        }
    }

    /// Record a trace context on the current span. Requires that the currently registered dispatcher
    /// have some 'TelemetryLayer' reachable via 'downcast_ref.
    pub fn record_on_current_span(self) -> Result<(), TraceCtxError> {
        let span = tracing::Span::current();
        let res = span
            .with_subscriber(|(current_span_id, dispatch)| {
                if let Some(layer) =
                    dispatch.downcast_ref::<crate::telemetry_layer::TraceCtxRegistry>()
                {
                    layer.record_trace_ctx(self, current_span_id.clone());
                    Ok(())
                } else {
                    Err(TraceCtxError::TelemetryLayerNotRegistered)
                }
            })
            .ok_or(TraceCtxError::NoEnabledSpan)?;
        res
    }

    /// get the current trace context (as registered on this node or a parent therof
    /// via 'record_on_current_span'). Requires that the currently registered dispatcher
    /// have some 'TelemetryLayer' reachable via 'downcast_ref.
    pub fn current_trace_ctx() -> Result<Self, TraceCtxError> {
        fn inner(x: (&tracing::Id, &tracing::Dispatch)) -> Result<TraceCtx, TraceCtxError> {
            let (current_span_id, dispatch) = x;
            let trace_ctx_registry = dispatch
                .downcast_ref::<crate::telemetry_layer::TraceCtxRegistry>()
                .ok_or(TraceCtxError::TelemetryLayerNotRegistered)?;

            let registry = dispatch
                .downcast_ref::<tracing_subscriber::Registry>()
                .ok_or(TraceCtxError::RegistrySubscriberNotRegistered)?;

            let iter = itertools::unfold(Some(current_span_id.clone()), |st| match st {
                Some(target_id) => {
                    // TODO: confirm panic is valid here
                    // failure here indicates a broken parent id span link, panic is valid
                    let res = registry
                        .span(target_id)
                        .expect("span data not found during eval_ctx for current_trace_ctx");
                    *st = res.parent().map(|x| x.id());
                    Some(res)
                }
                None => None,
            });

            trace_ctx_registry
                .eval_ctx(iter)
                .map(|x| TraceCtx {
                    parent_span: Some(trace_ctx_registry.span_id(current_span_id.clone())),
                    trace_id: x.trace_id,
                })
                .ok_or(TraceCtxError::NoParentNodeHasTraceCtx)
        }

        let span = tracing::Span::current();
        let res = span
            .with_subscriber(inner)
            .ok_or(TraceCtxError::NoEnabledSpan)?;
        res
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct SpanId {
    pub tracing_id: tracing::Id,
    pub instance_id: u64,
}

// TODO: round trip property test for this
impl SpanId {
    /// attempts to parse a SpanId from a string
    pub fn from_string(s: &str) -> Option<SpanId> {
        let mut iter = s.split('-');
        let s1 = iter.next()?;
        let u1 = u64::from_str_radix(s1, 10).ok()?;
        let s2 = iter.next()?;
        let u2 = u64::from_str_radix(s2, 10).ok()?;

        Some(SpanId {
            tracing_id: tracing::Id::from_u64(u1),
            instance_id: u2,
        })
    }

    /// convert a SpanId to a string that can be included in headers, RPC framework metadata fields, etc
    pub fn to_string(&self) -> String {
        format!("{}", self)
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.tracing_id.into_u64(), self.instance_id)
    }
}

/// NOTE: why not just have this be a u128? It'd be so much better...
/// A Honeycomb Trace ID. Uniquely identifies a single distributed (potentially multi-process) trace.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TraceId(pub(crate) u128);

impl TraceId {
    /// attempts to parse a TraceId from a string
    pub fn from_string(s: &str) -> Option<Self> {
        let u = u128::from_str_radix(s, 10).ok()?;
        Some(TraceId(u))
    }

    /// convert a TraceId to a string that can be included in headers, RPC framework metadata fields, etc
    pub fn to_string(&self) -> String {
        format!("{}", self.0)
    }

    /// Generate a random trace ID by using a thread-level RNG to generate a u128
    pub fn generate() -> Self {
        use rand::Rng;
        let u: u128 = rand::thread_rng().gen();

        TraceId(u)
    }
}

#[derive(Debug, Clone)]
pub struct Span<'a, V> {
    pub id: SpanId,
    pub trace_id: TraceId,
    pub parent_id: Option<SpanId>,
    pub initialized_at: DateTime<Utc>,
    pub elapsed_ms: i64,
    pub level: tracing::Level,
    pub name: &'a str,
    pub target: &'a str,
    pub service_name: &'a str,
    pub values: V, // visitor used to record values
}

impl<'a, V> Span<'a, V> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Span<'static, V> {
        Span {
            name: test::lift_to_static(self.name),
            target: test::lift_to_static(self.target),
            service_name: test::lift_to_static(self.service_name),
            id: self.id,
            trace_id: self.trace_id,
            parent_id: self.parent_id,
            initialized_at: self.initialized_at,
            elapsed_ms: self.elapsed_ms,
            level: self.level,
            values: self.values,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Event<'a, V> {
    pub trace_id: TraceId,
    pub parent_id: Option<SpanId>,
    pub initialized_at: DateTime<Utc>,
    pub level: tracing::Level,
    pub name: &'a str,
    pub target: &'a str,
    pub service_name: &'a str,
    pub values: V,
}

impl<'a, V> Event<'a, V> {
    #[cfg(test)]
    pub(crate) fn into_static(self) -> Event<'static, V> {
        Event {
            name: test::lift_to_static(self.name),
            target: test::lift_to_static(self.target),
            service_name: test::lift_to_static(self.service_name),
            trace_id: self.trace_id,
            parent_id: self.parent_id,
            initialized_at: self.initialized_at,
            level: self.level,
            values: self.values,
        }
    }
}

#[cfg(test)]
pub(crate) mod test {
    pub(super) fn lift_to_static(s: &'_ str) -> &'static str {
        use aovec::Aovec;
        lazy_static! {
            static ref STATIC_STRING_STORAGE: Aovec<String> = Aovec::new(256);
        }

        let idx = STATIC_STRING_STORAGE.push(s.to_string());
        STATIC_STRING_STORAGE.get(idx).unwrap()
    }
}
