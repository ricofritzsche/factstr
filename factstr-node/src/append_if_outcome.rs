use crate::{AppendResult, ConditionalAppendConflict};
use napi_derive::napi;

#[napi(object)]
#[derive(Clone, Debug, Default)]
pub struct AppendIfResult {
    #[napi(js_name = "append_result")]
    pub append_result: Option<AppendResult>,
    pub conflict: Option<ConditionalAppendConflict>,
}
