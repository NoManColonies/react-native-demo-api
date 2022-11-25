use tonic::{Code, Status};
use tracing::{error, warn};

use crate::app::util::sentry::*;
use sentry::capture_error as capture_exception;

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ServiceError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Redis(#[from] redis::RedisError),
    #[error("{0} middleware not set")]
    MiddlewareNotSet(&'static str),
    #[error("app config not set")]
    ConfigNotSet,
    #[error("Rc still has more than 0 reference(s). This is a bug")]
    RcHasReference,
    #[error("error: failed to parse error message: {0}")]
    ParseMessage(String),
    #[error("failed to validate request data: {field} reason: {reason}")]
    ValidateFailure { field: &'static str, reason: String },
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ParseUtf8(#[from] std::str::Utf8Error),
    #[error("bad credential")]
    BadCredential,
    #[error("rejected reason: {0}")]
    Rejected(String),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    #[error("failed to convert from '{field}' value '{from}' into '{into}' expecting {expect}")]
    TryFrom {
        field: &'static str,
        from: String,
        into: &'static str,
        expect: &'static str,
    },
    // #[error(transparent)]
    // Validation(#[from] validator::ValidationError),
    // #[error(transparent)]
    // Validations(#[from] validator::ValidationErrors),
    // #[error(transparent)]
    // Base64Decode(#[from] base64::DecodeError),
    #[error("empty value at index: {0}")]
    EmptySliceIndex(usize),
    #[error(transparent)]
    LapinAMQP(#[from] lapin::Error),
    #[error("failed to send a value")]
    SendError,
    #[error(transparent)]
    OneshotRecvError(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    TaskJoinError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    SemaphoreAquire(#[from] tokio::sync::AcquireError),
    #[error("amqp queue declare timeout")]
    QueueDeclareTimeout,
    #[error("amqp queue bind timeout")]
    QueueBindTimeout,
    #[error("amqp queue basic consume timeout")]
    QueueBasicConsumeTimeout,
    #[error("amqp queue basic ack timeout")]
    QueueBasicAckTimeout,
    #[error("client response timeout")]
    ClientTimeout,
    #[error(transparent)]
    CookieParse(#[from] cookie::ParseError),
    #[error(transparent)]
    HttpHeader(#[from] http::header::ToStrError),
    #[error("http header not found")]
    HttpHeaderNotFound,
    // #[error(transparent)]
    // AmqpTopicParseError(#[from] agripot_amqp_topic::error::ParseError),
    #[error(transparent)]
    MsgPackDecodeError(#[from] rmp_serde::decode::Error),
    #[error(transparent)]
    MsgPackEncodeError(#[from] rmp_serde::encode::Error),
    // #[error(transparent)]
    // SerializablePacket(#[from] agripot_serializable_packet::error::PacketError),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ServiceError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        ServiceError::SendError
    }
}

impl From<()> for ServiceError {
    fn from(_: ()) -> Self {
        ServiceError::SendError
    }
}

impl ServiceError {
    pub fn get_code(&self) -> Code {
        match self {
            Self::Reqwest(e) if e.is_body() => {
                warn!("reqwest body failed: {:?}", e);
                capture_warning("HTTP client failed to parse payload body");
                Code::InvalidArgument
            }
            Self::Reqwest(e) if e.is_builder() => {
                error!("reqwest builder failed: {:?}", e);
                capture_fatal("HTTP client failed to be initialized");
                Code::Internal
            }
            Self::Reqwest(e) if e.is_connect() => {
                error!("reqwest connection failed: {:?}", e);
                capture_error("HTTP client failed to initiate connection");
                Code::Unavailable
            }
            Self::Reqwest(e) if e.is_decode() => {
                warn!("reqwest decode failed: {:?}", e);
                capture_warning("HTTP client failed to decode payload instance");
                Code::FailedPrecondition
            }
            Self::Reqwest(e) if e.is_redirect() => {
                warn!("reqwest redirect failed: {:?}", e);
                capture_warning("HTTP client failed to perform redirection");
                Code::Internal
            }
            Self::Reqwest(e) if e.is_timeout() => {
                warn!("reqwest timed out: {:?}", e);
                capture_warning("HTTP client timed-out");
                Code::DeadlineExceeded
            }
            Self::Reqwest(e) if e.is_request() => {
                error!("bad reqwest: {:?}", e);
                capture_error("HTTP client threw internal exception");
                Code::FailedPrecondition
            }
            Self::Reqwest(e) if e.is_status() => {
                warn!("reqwest status failed: {:?}", e);
                capture_warning("HTTP client received bad protocol status");
                Code::FailedPrecondition
            }
            Self::Reqwest(e) => {
                error!("general reqwest error: {:?}", e);
                capture_error("HTTP client returned general failure");
                Code::Internal
            }
            Self::Redis(e) if e.is_timeout() => {
                warn!("redis timed out: {:?}", e);
                capture_warning("Redis client timed-out");
                Code::FailedPrecondition
            }
            Self::Redis(e) if e.is_cluster_error() => {
                error!("redis cluster failed: {:?}", e);
                capture_error("Redis client returned cluster related exception");
                Code::Unavailable
            }
            Self::Redis(e) if e.is_connection_dropped() => {
                warn!("redis connection dropped: {:?}", e);
                capture_warning("Redis client unexpectedly dropped connection");
                Code::Unavailable
            }
            Self::Redis(e) if e.is_connection_refusal() => {
                error!("redis connection refused: {:?}", e);
                capture_error("Redis client encountered connection refusal");
                Code::Unavailable
            }
            Self::Redis(e) if e.is_io_error() => {
                error!("redis io error: {:?}", e);
                capture_fatal("Redis client encountered network io failure");
                Code::Internal
            }
            Self::Redis(e) => {
                error!("general redis error: {:?}", e);
                capture_warning("Redis client returned internal exception");
                Code::Unavailable
            }
            Self::MiddlewareNotSet(e) => {
                error!("middleware not set: {:?}", e);
                capture_fatal("Service middleware was not properly setup");
                Code::Internal
            }
            Self::ConfigNotSet => {
                error!("config was not set");
                capture_fatal("Service configuration was not properly setup");
                Code::Internal
            }
            Self::ParseMessage(e) => {
                error!("error: failed to parse error message: {}", e);
                capture_error(
                    "Service encountered failure while attempting to parse error response message",
                );
                Code::Internal
            }
            Self::RcHasReference => {
                error!("{:?}", self);
                capture_fatal(
                    "Reference counted memory returned exception when attempting downgrade to non-reference counted memory",
                );
                Code::Internal
            }
            Self::ValidateFailure { .. } => Code::InvalidArgument,
            Self::ParseInt(e) => {
                warn!("failed integer parsing: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to parse input to the integer",
                );
                Code::InvalidArgument
            }
            Self::ParseUtf8(e) => {
                warn!("utf8 encoding failed: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to parse input to the UTF-8 encoded string",
                );
                Code::InvalidArgument
            }
            Self::BadCredential => Code::Unauthenticated,
            Self::Rejected(e) => {
                warn!("access rejected reason: {}", e);
                capture_warning(
                    "Incoming gRPC request attempted to access non-authorized resource",
                );
                Code::PermissionDenied
            }
            Self::Uuid(e) => {
                warn!("uuid parsing failure: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to parse input to UUID",
                );
                Code::InvalidArgument
            }
            Self::TryFrom { .. } => Code::DataLoss,
            // Self::Validation(_) => Code::InvalidArgument,
            // Self::Validations(_) => Code::InvalidArgument,
            // Self::Base64Decode(e) => {
            //     warn!("base64 decode failure: {:?}", e);
            //     capture_warning(
            //         "Service encountered failure from cryptography library `base64` failure to decode input",
            //     );
            //     Code::FailedPrecondition
            // }
            Self::EmptySliceIndex(i) => {
                warn!("empty slice index at: {}", i);
                capture_warning(
                    "Service encountered failure while attempting to access non-existing slice index",
                );
                Code::FailedPrecondition
            }
            Self::LapinAMQP(err) => {
                use lapin::Error;

                match err {
                    Error::ChannelsLimitReached => {
                        warn!("amqp channel limit reached");
                        capture_warning("AMQP client triggered channel limit exception");
                        Code::ResourceExhausted
                    }
                    Error::InvalidProtocolVersion(e) => {
                        error!("invalid amqp protocol version: {:?}", e);
                        capture_fatal("AMQP client triggered invalid protocol version exception");
                        Code::Internal
                    }
                    Error::InvalidChannel(e) => {
                        error!("invalid amqp channel: {:?}", e);
                        capture_error("AMQP client triggered invalid channel exception");
                        Code::FailedPrecondition
                    }
                    Error::InvalidChannelState(e) => {
                        error!("invalid amqp channel state: {:?}", e);
                        capture_error("AMQP client triggered invalid channel state exception");
                        Code::Unavailable
                    }
                    Error::InvalidConnectionState(e) => {
                        error!("invalid amqp connection state: {:?}", e);
                        capture_error("AMQP client triggered invalid connection state exception");
                        Code::Unavailable
                    }
                    Error::IOError(e) => {
                        error!("amqp io failure: {:?}", e);
                        capture_fatal("AMQP client triggered network IO exception");
                        Code::Internal
                    }
                    Error::ParsingError(e) => {
                        error!("amqp parse failure: {:?}", e);
                        capture_error("AMQP client triggered payload parsing exception");
                        Code::FailedPrecondition
                    }
                    Error::ProtocolError(e) => {
                        error!("amqp protocol failure: {:?}", e);
                        capture_fatal("AMQP client triggered protocol exception");
                        Code::Internal
                    }
                    Error::SerialisationError(e) => {
                        error!("amqp serialisation failure: {:?}", e);
                        capture_error("AMQP client triggered serialisation exception");
                        Code::FailedPrecondition
                    }
                    _ => {
                        error!("unknown amqp failure: {:?}", err);
                        capture_fatal("AMQP client triggered unknown exception");
                        Code::Internal
                    }
                }
            }
            Self::SendError => {
                warn!("tokio signal send error");
                capture_warning(
                    "Service encountered failure while attempting to send signal across asynchrous task",
                );
                Code::FailedPrecondition
            }
            Self::OneshotRecvError(e) => {
                warn!("tokio oneshot recv error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to receive signal from oneshot channel",
                );
                Code::FailedPrecondition
            }
            Self::TaskJoinError(e) if e.is_panic() => {
                error!("tokio task panicked: {:?}", e);
                {
                    capture_exception(e);
                    capture_fatal(
                        "Service encountered failure from unhandled exception inside asynchronous task",
                    );
                }
                Code::Internal
            }
            Self::TaskJoinError(e) if !e.is_cancelled() => {
                warn!("tokio task returned error: {:?}", e);
                capture_warning("Service encountered failure while computing asynchronous task");
                Code::Internal
            }
            Self::TaskJoinError(_) => Code::Cancelled,
            Self::SemaphoreAquire(e) => {
                warn!("semaphore acquire error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to acquire semaphore permit",
                );
                Code::FailedPrecondition
            }
            // total combined deadline time should not exceed retry count reset timer
            Self::QueueDeclareTimeout => Code::DeadlineExceeded,
            Self::QueueBindTimeout => Code::DeadlineExceeded,
            Self::QueueBasicConsumeTimeout => Code::DeadlineExceeded,
            Self::QueueBasicAckTimeout => Code::DeadlineExceeded,
            Self::ClientTimeout => Code::DeadlineExceeded,
            Self::CookieParse(e) => {
                warn!("cookie parse error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to parse http cookie",
                );
                Code::FailedPrecondition
            }
            Self::HttpHeader(e) => {
                warn!("http header error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to parse http header",
                );
                Code::FailedPrecondition
            }
            Self::HttpHeaderNotFound => {
                warn!("http header error");
                capture_warning(
                    "Service encountered failure while attempting to access request header",
                );
                Code::Internal
            }
            // Self::AmqpTopicParseError(e) => {
            //     warn!("amqp topic parse error: {:?}", e);
            //     capture_warning("Service encountered failure while attempting to parse amqp topic");
            //     Code::FailedPrecondition
            // }
            Self::MsgPackDecodeError(e) => {
                warn!("msgpack decode error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to decode msgpack packet",
                );
                Code::FailedPrecondition
            }
            Self::MsgPackEncodeError(e) => {
                warn!("msgpack encode error: {:?}", e);
                capture_warning(
                    "Service encountered failure while attempting to encode msgpack packet",
                );
                Code::FailedPrecondition
            }
            // Self::SerializablePacket(e) => {
            //     warn!("serializable packet error: {:?}", e);
            //     capture_warning(
            //         "Service encountered failure while attempting to operate on serializable packet",
            //     );
            //     Code::FailedPrecondition
            // }
            #[allow(unreachable_patterns)]
            _ => {
                error!("undocumented error: {}", &self.to_string());
                capture_fatal("Service encountered exception from unknown source");
                Code::Unknown
            }
        }
    }
}

impl From<ServiceError> for Status {
    fn from(error: ServiceError) -> Self {
        Status::new(error.get_code(), error.to_string())
    }
}
