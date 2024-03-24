use crate::server::main_grpc::{
    raft_client::RaftClient, HeartbeatReply, HeartbeatRequest, RequestVoteReply, RequestVoteRequest,
};

use tonic::{transport::Channel, IntoRequest, Response, Status};

// Inspired from https://github.com/TraceMachina/nativelink/blob/90cff230ebb5e7982d780f767aa0b0dc85d87b20/cas/worker/worker_api_client_wrapper.rs#L38
pub(crate) trait RaftClientTrait: Clone + Sync + Send + Sized + Unpin {
    async fn heartbeat(
        &mut self,
        request: impl IntoRequest<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status>;
    async fn request_vote(
        &mut self,
        request: impl IntoRequest<RequestVoteRequest>,
    ) -> Result<Response<RequestVoteReply>, Status>;
}

impl RaftClientTrait for RaftClient<Channel> {
    async fn heartbeat(
        &mut self,
        request: impl IntoRequest<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatReply>, Status> {
        self.heartbeat(request).await
    }

    async fn request_vote(
        &mut self,
        request: impl IntoRequest<RequestVoteRequest>,
    ) -> Result<Response<RequestVoteReply>, Status> {
        self.request_vote(request).await
    }
}
