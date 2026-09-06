use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{ClientJsonRpcMessage, ClientRequest, ProtocolVersion, ServerJsonRpcMessage},
    transport::{TokioChildProcess, Transport},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Counted<T> {
    inner: T,
    lists: Arc<AtomicUsize>,
    inventories: Arc<AtomicUsize>,
    reads: Arc<AtomicUsize>,
}
impl<T: Transport<RoleClient>> Transport<RoleClient> for Counted<T> {
    type Error = T::Error;
    fn send(
        &mut self,
        item: ClientJsonRpcMessage,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send + 'static {
        if matches!(&item,ClientJsonRpcMessage::Request(r) if matches!(&r.request,ClientRequest::ListToolsRequest(_)))
        {
            self.lists.fetch_add(1, Ordering::SeqCst);
        }
        if matches!(&item,ClientJsonRpcMessage::Request(r) if matches!(&r.request,ClientRequest::ListResourcesRequest(_)))
        {
            self.inventories.fetch_add(1, Ordering::SeqCst);
        }
        if matches!(&item,ClientJsonRpcMessage::Request(r) if matches!(&r.request,ClientRequest::ReadResourceRequest(_)))
        {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.send(item)
    }
    async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
        self.inner.receive().await
    }
    async fn close(&mut self) -> Result<(), Self::Error> {
        self.inner.close().await
    }
}
#[tokio::test]
async fn official_client_discovers_initializes_and_caches_only_inside_its_peer() {
    tokio::time::timeout(std::time::Duration::from_secs(30),async {
        for modern in [true,false,true] {
            let root=tempfile::tempdir().unwrap();
            let mut command=tokio::process::Command::new(env!("CARGO_BIN_EXE_krometrail"));
            command.arg("mcp").env("KROMETRAIL_DATA_DIR",root.path().join("data")).env("KROMETRAIL_FFMPEG_PATH",root.path().join("missing"));
            let count=Arc::new(AtomicUsize::new(0));
            let inventories=Arc::new(AtomicUsize::new(0));
            let reads=Arc::new(AtomicUsize::new(0));
            let transport=Counted{inner:TokioChildProcess::new(command).unwrap(),lists:count.clone(),inventories:inventories.clone(),reads:reads.clone()};
            let mode=if modern {ClientLifecycleMode::Discover{preferred_versions:vec![ProtocolVersion::V_2026_07_28]}}else{ClientLifecycleMode::Initialize};
            let client=().serve_with_lifecycle(transport,mode).await.unwrap();
            let first=client.list_tools(None).await.unwrap();
            assert_eq!(count.load(Ordering::SeqCst),1,"new Peer must fetch its own catalogue");
            let second=client.list_tools(None).await.unwrap();assert_eq!(first.tools,second.tools);
            assert_eq!(count.load(Ordering::SeqCst),if modern{1}else{2});
            let all=client.list_all_tools().await.unwrap();assert!(all.len()>8);
            if !modern { assert_eq!(first.tools,all); assert!(first.next_cursor.is_none()); }
            let result=client.call_tool(rmcp::model::CallToolRequestParams::new("list_managed_profiles")).await.unwrap();assert_eq!(result.is_error,Some(false));
            // Keep the SDK cache enabled: successful zero-TTL inventories must be
            // revalidated, and failed reads must not be satisfied from a stale cache.
            for _ in 0..2 { assert!(client.list_resources(None).await.unwrap().resources.is_empty()); }
            assert_eq!(inventories.load(Ordering::SeqCst),2);
            for _ in 0..2 {
                assert!(client.read_resource(rmcp::model::ReadResourceRequestParams::new("krometrail://evidence/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002/frames/00000000-0000-0000-0000-000000000003")).await.is_err());
            }
            assert_eq!(reads.load(Ordering::SeqCst),2);
            client.cancel().await.unwrap();
        }
    }).await.unwrap();
}
