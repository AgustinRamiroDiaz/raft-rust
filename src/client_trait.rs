use std::future::Future;

use crate::server::main_grpc::{
    raft_client::RaftClient, HeartbeatReply, HeartbeatRequest, RequestVoteReply, RequestVoteRequest,
};

use tonic::{transport::Channel, IntoRequest, Response, Status};

// Inspired from https://github.com/TraceMachina/nativelink/blob/90cff230ebb5e7982d780f767aa0b0dc85d87b20/cas/worker/worker_api_client_wrapper.rs#L38
pub(crate) trait RaftClientTrait {
    fn heartbeat(
        &mut self,
        request: impl IntoRequest<HeartbeatRequest> + Send,
    ) -> impl Future<Output = Result<Response<HeartbeatReply>, Status>> + Send;
    fn request_vote(
        &mut self,
        request: impl IntoRequest<RequestVoteRequest> + Send,
    ) -> impl Future<Output = Result<Response<RequestVoteReply>, Status>> + Send;
}

impl RaftClientTrait for RaftClient<Channel> {
    fn heartbeat(
        &mut self,
        request: impl IntoRequest<HeartbeatRequest> + Send,
    ) -> impl Future<Output = Result<Response<HeartbeatReply>, Status>> + Send {
        self.heartbeat(request)
    }

    fn request_vote(
        &mut self,
        request: impl IntoRequest<RequestVoteRequest> + Send,
    ) -> impl Future<Output = Result<Response<RequestVoteReply>, Status>> + Send {
        self.request_vote(request)
    }
}
