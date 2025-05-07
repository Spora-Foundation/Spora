use tondi_grpc_core::protowire::{tondid_request, tondid_response, TondidRequest, TondidResponse};

pub(crate) trait Matcher<T> {
    fn is_matching(&self, response: T) -> bool;
}

impl Matcher<&tondid_response::Payload> for tondid_request::Payload {
    fn is_matching(&self, response: &tondid_response::Payload) -> bool {
        use tondid_request::Payload;
        match self {
            // TODO: implement for each payload variant supporting request/response pairing
            Payload::GetBlockRequest(ref request) => {
                if let tondid_response::Payload::GetBlockResponse(ref response) = response {
                    if let Some(block) = response.block.as_ref() {
                        if let Some(verbose_data) = block.verbose_data.as_ref() {
                            return verbose_data.hash == request.hash;
                        }
                        return true;
                    } else if let Some(error) = response.error.as_ref() {
                        // the response error message should contain the requested hash
                        return error.message.contains(request.hash.as_str());
                    }
                }
                false
            }

            _ => true,
        }
    }
}

impl Matcher<&TondidResponse> for TondidRequest {
    fn is_matching(&self, response: &TondidResponse) -> bool {
        if let Some(ref response) = response.payload {
            if let Some(ref request) = self.payload {
                return request.is_matching(response);
            }
        }
        false
    }
}
