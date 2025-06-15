# conduitの技術設計

ステータス: 完成
担当者: ひでまる
最終更新日: 2025-06-14 22:38 JST
LICENSE: Apache 2.0 LICENSE and SUSHI-WARE
バージョン: v1.0

# Conduit - ネットワークトンネリングソフトウェア 最終仕様書

## 目次

1. プロダクト概要
2. アーキテクチャ設計
3. コマンドライン仕様
4. 設定管理システム
5. セキュリティ仕様
6. コア機能実装
7. 追加機能実装
8. エラーハンドリング
9. ログ・監視システム
10. パフォーマンス仕様
11. デプロイメント戦略
12. API仕様
13. プラットフォーム対応
14. テスト戦略
15. 実装ガイドライン
16. 付録

---

## プロダクト概要

ConduitはRust製のネットワークトンネリングソフトウェアです。**外部ユーザーからのアクセスをClient経由でRouter同一サブネット内のサービスに安全かつ高性能に転送**します。主にプライベートネットワーク内のサービスにインターネット経由でアクセスする用途で利用されます。

**主要特徴**:

- **高性能**: Rust + Tokioによる非同期処理
- **セキュア**: Client-Router間をTLS 1.3 + Ed25519で暗号化
- **スケーラブル**: 数万同時接続対応
- **運用性**: Docker/Kubernetesネイティブ対応
- **可観測性**: 包括的な監視・ログ機能
- **単一バイナリ**: ClientとRouter機能を統合

**設計哲学**:

- Docker/Compose風UX: 単発実行と設定ファイルベース実行
- 階層的設定: CLI > 環境変数 > 設定ファイル > デフォルト
- ヘッドレス対応: CI/CD、Docker、Kubernetesでの自動化

**ユースケース**:

- **リモートサーバーへの安全なアクセス**: プライベートネットワーク内のDBやAPIサーバーにインターネット経由でアクセス。
- **開発環境への外部アクセス**: 外部の共同作業者がClient経由でRouter側の開発サーバーにアクセス。
- **プライベートクラウドリソースへの接続**: NAT越しでRouter側のクラウドサービスやマイクロサービスに接続。
- **セキュアなリバースプロキシ**: Router側のサービス群への暗号化されたアクセス経路を提供。

---

## アーキテクチャ設計

**デーモンレス設計**: ConduitはPodmanライクなデーモンレスアーキテクチャを採用し、中央管理デーモンを持たず、独立したトンネルプロセスによる分散管理を実現します。

### システム構成

```
外部ネットワークユーザー (例: Browser)
    │
    │ (Tunnel Process の bind ポート経由でアクセス)
    ▼
┌─────────────────────────────────┐      ┌─────────────────────────────┐
│        Client Side              │      │        Router Side          │
│                                 │      │                             │
│  ┌─────────────┐  ┌─────────────┐│      │ ┌─────────────────────────┐ │
│  │ CLI         │  │ Process     ││      │ │       Router            │ │
│  │ Commands    │  │ Registry    ││      │ │   :9999 (待受)          │ │
│  └─────────────┘  └─────────────┘│      │ │                         │ │
│         │                │       │      │ └─────────────────────────┘ │
│         │ gRPC           │ File  │      │             │               │
│         ▼                ▼       │      │             ▼               │
│  ┌─────────────────────────────┐ │ TLS  │ ┌─────────────────────────┐ │
│  │    Tunnel Process          │ │◄────►│ │    Target Service       │ │
│  │  :80 (bind待受)            │ │      │ │  :8080 (source)         │ │
│  │  :50001 (gRPC)             │ │      │ └─────────────────────────┘ │
│  └─────────────────────────────┘ │      │                             │
└─────────────────────────────────┘      └─────────────────────────────┘
```

### 主要コンポーネント

#### 1. CLI Commands
- **役割**: ユーザーインターフェースとして動作
- **通信**: gRPC経由でTunnel Processと通信
- **コマンド**: [`conduit start`](src/cli/commands/start.rs), [`conduit up`](src/cli/commands/up.rs), [`conduit list`](src/cli/commands/list.rs), [`conduit kill`](src/cli/commands/kill.rs), [`conduit status`](src/cli/commands/status.rs)

#### 2. Tunnel Process
- **役割**: 独立したプロセスとしてトンネルを管理・実行
- **機能**:
  - 外部ユーザーからのアクセスを受け付け（`--bind`ポート）
  - TLS暗号化によるRouterとの安全な通信
  - gRPCサーバーとしてCLIコマンドからの制御要求を受け付け
- **ライフサイクル**: `conduit start`/`conduit up`で起動、明示的な終了まで動作継続

#### 3. Process Registry
- **役割**: 実行中のTunnel Processの情報を管理
- **実装**: ファイルベースのレジストリ（`~/.conduit/tunnels/`）
- **情報**: プロセスID、gRPCアドレス、トンネル設定、状態情報

#### 4. Router
- **役割**: プライベートネットワーク内の中継サーバー
- **機能**: Tunnel Processからの要求を受け付け、認証・ルーティングを実行

### データフロー

#### 1. トンネル確立フロー
```
conduit start --router 10.2.0.1:9999 --source 10.2.0.2:8080 --bind 0.0.0.0:80
    ↓
1. Tunnel Processをバックグラウンドで起動
2. Process RegistryにTunnel Process情報を登録
3. Tunnel ProcessがgRPCサーバーを起動 (例: :50001)
4. Tunnel ProcessがRouterにTLS接続・認証
5. トンネル確立成功、外部アクセス待機開始
```

#### 2. データ転送フロー
```
外部ユーザー → Tunnel Process(:80) → Router → Target Service(:8080)
                    ↑ TLS暗号化 ↑
```

#### 3. 制御コマンドフロー
```
conduit list
    ↓
1. Process Registryから実行中のTunnel Processを検索
2. 各Tunnel ProcessのgRPCエンドポイントに接続
3. GetStatus RPCでトンネル情報を取得
4. 結果をユーザーに表示
```

### gRPC API設計

#### TunnelControl Service
```protobuf
service TunnelControl {
  rpc GetStatus(StatusRequest) returns (StatusResponse);
  rpc ListConnections(ListRequest) returns (ListResponse);
  rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
  rpc GetMetrics(MetricsRequest) returns (MetricsResponse);
}

message TunnelInfo {
  string id = 1;
  string name = 2;
  string router_addr = 3;
  string source_addr = 4;
  string bind_addr = 5;
  string status = 6;
  int64 created_at = 7;
  int32 active_connections = 8;
}
```

### Process Registry形式
```json
{
  "tunnel_id": "web-server-access-1234",
  "name": "web-server-access",
  "pid": 12345,
  "grpc_port": 50001,
  "config": {
    "router": "10.2.0.1:9999",
    "source": "10.2.0.2:8080",
    "bind": "0.0.0.0:80"
  },
  "status": "running",
  "created_at": "2025-06-15T03:35:00Z"
}
```

### サービスファイル生成機能

#### コマンド仕様
```bash
# systemdサービスファイル生成
conduit start --service-file systemd --router 10.2.0.1:9999 --source 10.2.0.2:8080 --bind 0.0.0.0:80

# 複数プラットフォーム対応
conduit up --service-file launchd -f conduit.toml
conduit up --service-file openrc -f conduit.toml
conduit up --service-file rc.d -f conduit.toml
```

#### 対応プラットフォーム
- **systemd** (Linux)
- **openrc** (Alpine Linux)
- **launchd** (macOS)
- **rc.d** (FreeBSD)

---

## コマンドライン仕様

**基本**: `conduit <SUBCOMMAND> [OPTIONS]`

**全コマンド一覧**:
| コマンド | 説明 | 主な用途 | 実行モード |
|---|---|---|---|
| `init` | 初期化・キーペア生成 | 初回設定 | 単発実行 |
| `start` | 単発トンネル開始 | Client経由でRouter側サービスへのアクセス経路を作成 | Client |
| `up` | 設定ファイルから一括起動 | 複数のRouter側サービスへのアクセス経路を提供 | Client |
| `down` | 設定ファイルのトンネル停止 | `up`で起動したトンネルを一括停止 | Client制御 |
| `router` | ルーター起動 | Routerサーバーの運用 | Router |
| `list` | トンネル・接続一覧表示 | アクティブなトンネルと接続状況の確認 | 制御コマンド |
| `kill` | トンネル・接続終了 | 特定のトンネルまたは接続の強制終了 | 制御コマンド |
| `status` | システム状況確認 | Router、Clientの稼働状況確認 | 制御コマンド |
| `config` | 設定管理 | 設定の表示・検証・生成 | ユーティリティ |
| `version` | バージョン情報表示 | Conduitのバージョンとビルド情報表示 | ユーティリティ |

**使用例**:

```bash
# 初期化: Client/Routerで使用するキーペアを生成
conduit init

# 単発トンネル開始 (ClientとしてRouter側サービスへのアクセス経路を作成)
conduit start --router 10.2.0.1:9999 --source 10.2.0.2:8080 --bind 0.0.0.0:80

# 設定ファイルから複数のRouter側サービスへのアクセス経路を作成
conduit up -f conduit.toml

# 設定ファイルのトンネル停止
conduit down -f conduit.toml

# ルーター単体起動
conduit router --bind 0.0.0.0:9999

# 運用管理: システム状況確認
conduit status

# 運用管理: アクティブなトンネルと接続の確認
conduit list

# 運用管理: 特定のトンネル終了
conduit kill --tunnel web-server-access

# 運用管理: 全トンネル終了
conduit kill --all

# 設定確認
conduit config validate
conduit config show

# 環境別設定ファイルの使い分け
conduit up -f conduit-dev.toml     # 開発環境
conduit up -f conduit-prod.toml    # 本番環境
conduit up -f conduit-staging.toml # ステージング環境

# サービスファイル生成
conduit start --service-file systemd --router 10.2.0.1:9999 --source 10.2.0.2:8080 --bind 0.0.0.0:80
conduit up --service-file launchd -f conduit.toml
```

---

## 設定管理システム

**優先順位**: CLI引数 > 環境変数 > 設定ファイル (`conduit.toml`) > デフォルト

**設定ファイル例** (`conduit.toml` で `conduit up` を実行する場合):

```toml
# Clientが接続するRouterサーバーの情報
[router]
host = "10.2.0.1"           # Router のIPアドレス (Router同一サブネット内)
port = 9999                 # Routerの待ち受けポート

# Clientのセキュリティ設定
[security]
private_key_path = "./keys/client.key" # Clientの秘密鍵

# Router側サービスへのアクセス経路設定
[[tunnels]]
name = "web-server-access"     # トンネルの識別名
source = "10.2.0.2:8080"      # Router側サービスのアドレスとポート
bind = "0.0.0.0:80"           # Clientがこのアドレスとポートで外部接続を待ち受ける
# protocol = "tcp" (デフォルト) / "udp"

[[tunnels]]
name = "api-server-access"
source = "10.2.0.3:3000"      # 別のRouter側サービス
bind = "0.0.0.0:8080"
```

**環境変数**: `CONDUIT_<SECTION>_<KEY>` (例: `CONDUIT_ROUTER_HOST`)
**環境別管理**: 設定ファイルを分けることで環境ごとの設定を管理 (例: `conduit-dev.toml`, `conduit-prod.toml`)

---

## セキュリティ仕様

**暗号化**: TLS 1.3 + Ed25519 (32バイトキー, 128bit相当)
**キー管理**:

- Ed25519キーペア: `conduit init`で自動生成 (`client.key`, `client.pub`)
- 鍵ローテーション: 30日間隔、24時間グレースピリオド (Router制御)
  **認証・認可**: 相互TLS認証、トークンベース認可
  (詳細は `docs/security-guide.md` 参照)

---

## コア機能実装

**技術スタック**:

- **基盤**: Rust + Tokio (非同期), rustls (TLS), ed25519-dalek
- **gRPC**: tonic, prost (プロトコルバッファ)
- **設定/CLI**: serde, toml, clap
- **監視**: prometheus, tracing

### デーモンレスアーキテクチャ実装

#### 1. Tunnel Process (`src/tunnel/`)
```rust
// トンネルプロセスのメイン構造
pub struct TunnelProcess {
    id: String,
    config: TunnelConfig,
    grpc_server: TunnelControlServer,
    tunnel_manager: TunnelManager,
    registry: ProcessRegistry,
}

pub struct TunnelManager {
    router_connection: RouterConnection,
    bind_listener: TcpListener,
    active_connections: HashMap<String, ConnectionInfo>,
    metrics: TunnelMetrics,
}
```

#### 2. Process Registry (`src/registry/`)
```rust
// ファイルベースのプロセス情報管理
pub struct ProcessRegistry {
    registry_dir: PathBuf, // ~/.conduit/tunnels/
}

#[derive(Serialize, Deserialize)]
pub struct ProcessInfo {
    tunnel_id: String,
    name: String,
    pid: u32,
    grpc_port: u16,
    config: TunnelConfig,
    status: ProcessStatus,
    created_at: DateTime<Utc>,
}
```

#### 3. gRPC Control Service (`src/grpc/`)
```rust
// CLI ↔ Tunnel Process間のgRPC通信
#[tonic::async_trait]
impl TunnelControl for TunnelControlService {
    async fn get_status(&self, request: Request<StatusRequest>)
        -> Result<Response<StatusResponse>, Status>;
    
    async fn list_connections(&self, request: Request<ListRequest>)
        -> Result<Response<ListResponse>, Status>;
    
    async fn shutdown(&self, request: Request<ShutdownRequest>)
        -> Result<Response<ShutdownResponse>, Status>;
}
```

#### 4. CLI Commands (`src/cli/commands/`)
```rust
// gRPCクライアントを使用したコマンド実装
pub async fn execute_list(args: ListArgs) -> CommandResult {
    let registry = ProcessRegistry::new()?;
    let processes = registry.list_active_processes()?;
    
    for process in processes {
        let client = TunnelControlClient::connect(
            format!("http://127.0.0.1:{}", process.grpc_port)
        ).await?;
        
        let status = client.get_status(StatusRequest {}).await?;
        // 結果表示処理
    }
}
```

### ディレクトリ構造

```
src/
├── grpc/
│   ├── mod.rs
│   ├── server.rs       # gRPCサーバー実装
│   ├── client.rs       # gRPCクライアント実装
│   └── tunnel.proto    # プロトコル定義
├── registry/
│   ├── mod.rs
│   ├── manager.rs      # レジストリ管理
│   └── process.rs      # プロセス情報
├── tunnel/
│   ├── mod.rs
│   ├── process.rs      # トンネルプロセス
│   └── manager.rs      # トンネル管理
├── service/
│   ├── mod.rs
│   ├── template.rs     # サービスファイルテンプレート
│   └── generator.rs    # サービスファイル生成
└── cli/commands/       # 既存のCLIコマンド実装
```

**パフォーマンス目標**: 10,000+同時接続, 10Gbps+スループット, <10msレイテンシ
**最適化**: ゼロコピーI/O, 接続プール, 自動チューニング

**メッセージフレーミング**:

```rust
// bincodeシリアライズ + 4バイト長プレフィックス
async fn read_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<TunnelMessage>;
fn encode_message(message: &TunnelMessage) -> Result<Vec<u8>>;
```

---

## 追加機能実装

**P1 (必須)**:

- **ヘルスチェック・監視**:
  - Routerから指定Router側サービス (`--source` で指定されたRouter同一サブネット内のサービス) への定期的な疎通確認。
  - RouterとClient間のトンネル接続状態の監視。
  - 接続失敗時のアラート送信 (Webhook/Slack統合)。
- **REST API (Router)**: Client接続情報、トラフィック統計、Router監視情報。

**P2 (拡張)**:

- **トラフィック解析**: リアルタイム通信量監視、接続統計の収集・保存、パフォーマンス分析レポート。
- **高度なルーティング**:
  - **ロードバランシング**:
    - 同一の設定を持つ複数のConduit Clientインスタンス間でのRouterレベルでの負荷分散。
    - (将来検討) Clientが `--source` で複数のRouter側サービスエンドポイントを指定した場合のRouter内部での負荷分散。
  - **自動フェイルオーバー**: 上記ロードバランシング対象のClientインスタンスまたはRouter側サービスエンドポイント障害時の自動切り替え。
  - **サーキットブレーカー機能**: 障害発生時の過度なリトライ防止と迅速な復旧支援。

---

## エラーハンドリング

**主要エラー型**: `ConfigError`, `NetworkError`, `TlsError`, `AuthenticationError`, `TunnelError` (詳細は `src/error.rs`)
**リトライ戦略**: 最大3回、指数バックオフ (100msベース)、ジッター

---

## ログ・監視システム

**構造化ログ**: JSON (本番), Text (開発), ファイルローテーション
**ログレベル**: `error`, `warn`, `info`, `debug`, `trace`
**メトリクス**: Prometheus形式 (`/metrics` エンドポイント)

- `conduit_connections_total`, `conduit_bytes_transferred`, etc. (詳細は `metrics.md`)

---

## パフォーマンス仕様

**性能目標**: 10,000+同時接続, 10Gbps+スループット, <10msレイテンシ, CPU <50% (8コア), メモリ <1GB
**設定項目** (`PerformanceConfig`):

- 自動チューニング (`auto_tune`, `optimize_for: Throughput|Latency|Balanced`)
- 接続プール (`connection_pool_size`, timeouts)
- バッファサイズ (`read/write_buffer_size`), `tcp_nodelay`, `tcp_keepalive`
- 帯域制限 (`default/burst_bandwidth_limit`)
  **最適化手法**: Zero-copy, 接続プール, 適応的バッファ, CPU親和性

---

## デプロイメント戦略

**Docker**:

```dockerfile
# Multi-stage build: builder (Rust) -> final (Alpine)
FROM rust:alpine AS builder; WORKDIR /app; COPY . .; RUN cargo build --release
FROM alpine:latest; COPY --from=builder /app/target/release/conduit /usr/local/bin/; ENTRYPOINT ["conduit"]
```

```yaml
# docker-compose.yml example
services:
  router: { image: conduit, command: router }
  client: { image: conduit, command: up }
```

**Kubernetes**: `Deployment`, `Service`, `ConfigMap`, `Secret` (詳細は `k8s/`参照)
**Systemd**:

```ini
[Unit] Description=Conduit; [Service] ExecStart=/usr/local/bin/conduit up -f /etc/conduit/config.toml; Restart=always; [Install] WantedBy=multi-user.target
```

---

## API仕様

### Router Statistics API (`http://router-ip:9999/api/v1`)

**統計エンドポイント**:

- 基本: `GET /info`, `/health`, `/metrics`
- Client情報: `GET /clients`, `GET /clients/{id}`
- 統計情報: `GET /stats/traffic`, `/stats/connections`

**レスポンス形式** (JSON):

```json
{
  "success": true,
  "data": {},
  "error": null,
  "timestamp": "...",
  "request_id": "..."
}
```

ページネーション対応。エラー時は `error` に詳細格納。

---

## プラットフォーム対応

### サポートOS

- **Linux**: Ubuntu 20.04+, CentOS 8+, Alpine Linux, Debian 11+
- **macOS**: 10.15+ (Intel/Apple Silicon)
- **Windows**: Windows 10+ (ネイティブ対応、WSL2推奨)

### アーキテクチャ

- **x86_64**: 完全サポート
- **ARM64**: 完全サポート (Apple Silicon, AWS Graviton)
- **ARMv7**: 限定サポート (Raspberry Pi)

### パッケージング

- **バイナリリリース**: GitHub Releases (複数プラットフォーム)
- **コンテナイメージ**: Docker Hub, GitHub Container Registry
- **パッケージマネージャー**: Homebrew (macOS), APT (Ubuntu/Debian), YUM (RHEL/CentOS)

---

## テスト戦略

### テスト種別

| 種別                     | 対象               | カバレッジ目標     |
| ------------------------ | ------------------ | ------------------ |
| **単体テスト**           | 各モジュール       | 90%+               |
| **統合テスト**           | E2Eシナリオ        | 主要機能100%       |
| **パフォーマンステスト** | 負荷・スループット | ベンチマーク       |
| **セキュリティテスト**   | 脆弱性・暗号化     | 全セキュリティ機能 |

### 主要テストケース

**基本機能**:

- トンネル作成・切断
- 設定ファイル解析
- キーペア生成・認証

**統合テスト**:

- エンドツーエンド通信
- マルチトンネル処理
- 異常系・復旧処理

**テスト実行**:

```bash
# 全テスト実行
cargo test --all-features

# カバレッジ測定
cargo tarpaulin --all-features --workspace
```

詳細なテストコードとシナリオは [`docs/testing-guide.md`](testing-guide.md) を参照。

### セキュリティ統合テスト

```rust
#[tokio::test]
async fn test_security_integration() {
    // 証明書とキーの生成
    let (server_cert, server_key) = generate_test_cert().unwrap();
    let (client_cert, client_key) = generate_test_cert().unwrap();

    // セキュアルーター起動
    let router = SecureTestRouter::start_with_certs(
        "127.0.0.1:39999",
        server_cert,
        server_key
    ).await.unwrap();

    // 認証付きクライアント
    let client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:39999".parse().unwrap(),
        source_addr: "127.0.0.1:38080".parse().unwrap(),
        bind_addr: "127.0.0.1:37070".parse().unwrap(),
        tls_config: Some(TlsConfig {
            cert: client_cert,
            key: client_key,
            ca_cert: Some(server_cert.clone()),
        }),
        ..Default::default()
    });

    // 接続とデータ転送のテスト
    client.start().await.unwrap();

    // TLS接続の検証
    let connection_info = client.get_connection_info().await.unwrap();
    assert!(connection_info.is_encrypted);
    assert_eq!(connection_info.tls_version, "1.3");

    client.shutdown().await.unwrap();
    router.shutdown().await.unwrap();
}

```

### パフォーマンステスト

**概要**: 負荷・性能・スケーラビリティテスト

**負荷テスト**:

```rust
#[tokio::test]
async fn test_high_connection_load() {
    let router = TestRouter::start("127.0.0.1:49999").await.unwrap();
    let target = TestServer::start("127.0.0.1:48080").await.unwrap();

    let client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:49999".parse().unwrap(),
        source_addr: "127.0.0.1:48080".parse().unwrap(),
        bind_addr: "127.0.0.1:47070".parse().unwrap(),
        max_connections: 10000,
        ..Default::default()
    });

    client.start().await.unwrap();

    // 1000個の同時接続テスト
    let mut handles = Vec::new();
    for i in 0..1000 {
        let handle = tokio::spawn(async move {
            let mut stream = TcpStream::connect("127.0.0.1:47070").await.unwrap();
            stream.write_all(format!("test message {}", i).as_bytes()).await.unwrap();

            let mut buffer = [0; 1024];
            let _n = stream.read(&mut buffer).await.unwrap();

            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        handles.push(handle);
    }

    // 全ての接続が完了することを確認
    for handle in handles {
        handle.await.unwrap();
    }

    // メトリクス確認
    let metrics = client.get_metrics().await.unwrap();
    assert!(metrics.successful_connections >= 1000);
    assert!(metrics.error_rate < 0.01); // 1%未満のエラー率

    client.shutdown().await.unwrap();
}

```

**スループットテスト**:

```rust
#[tokio::test]
async fn test_throughput_benchmark() {
    let router = TestRouter::start("127.0.0.1:59999").await.unwrap();
    let target = HighThroughputTestServer::start("127.0.0.1:58080").await.unwrap();

    let client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:59999".parse().unwrap(),
        source_addr: "127.0.0.1:58080".parse().unwrap(),
        bind_addr: "127.0.0.1:57070".parse().unwrap(),
        performance: PerformanceConfig {
            optimize_for: OptimizationTarget::Throughput,
            read_buffer_size: 1024 * 1024, // 1MB
            write_buffer_size: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    });

    client.start().await.unwrap();

    // 大量データ転送テスト
    let data_size = 1024 * 1024 * 100; // 100MB
    let test_data = vec![0xAA; data_size];

    let start_time = Instant::now();

    let mut stream = TcpStream::connect("127.0.0.1:57070").await.unwrap();
    stream.write_all(&test_data).await.unwrap();
    stream.flush().await.unwrap();

    let mut received = Vec::new();
    let mut buffer = [0; 8192];
    while received.len() < data_size {
        let n = stream.read(&mut buffer).await.unwrap();
        received.extend_from_slice(&buffer[..n]);
    }

    let elapsed = start_time.elapsed();
    let throughput = (data_size as f64 / elapsed.as_secs_f64()) / (1024.0 * 1024.0); // MB/s

    println!("Throughput: {:.2} MB/s", throughput);
    assert!(throughput > 100.0); // 100MB/s以上

    client.shutdown().await.unwrap();
}

```

**レイテンシテスト**:

```rust
#[tokio::test]
async fn test_latency_benchmark() {
    let router = TestRouter::start("127.0.0.1:69999").await.unwrap();
    let target = LowLatencyTestServer::start("127.0.0.1:68080").await.unwrap();

    let client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:69999".parse().unwrap(),
        target_addr: "127.0.0.1:68080".parse().unwrap(),
        bind_addr: "127.0.0.1:67070".parse().unwrap(),
        performance: PerformanceConfig {
            optimize_for: OptimizationTarget::Latency,
            tcp_nodelay: true,
            ..Default::default()
        },
        ..Default::default()
    });

    client.start().await.unwrap();

    let mut latencies = Vec::new();

    // 1000回のping-pongテスト
    for _ in 0..1000 {
        let mut stream = TcpStream::connect("127.0.0.1:67070").await.unwrap();

        let start = Instant::now();
        stream.write_all(b"ping").await.unwrap();

        let mut buffer = [0; 4];
        stream.read_exact(&mut buffer).await.unwrap();
        let latency = start.elapsed();

        latencies.push(latency);

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let p95_latency = latencies[latencies.len() * 95 / 100];

    println!("Average latency: {:?}", avg_latency);
    println!("P95 latency: {:?}", p95_latency);

    assert!(avg_latency < Duration::from_millis(10)); // 平均10ms未満
    assert!(p95_latency < Duration::from_millis(20));  // P95で20ms未満

    client.shutdown().await.unwrap();
}

```

### セキュリティテスト

**概要**: セキュリティ脆弱性とペネトレーションテスト

**認証テスト**:

```rust
#[tokio::test]
async fn test_authentication_security() {
    let router = SecureTestRouter::start("127.0.0.1:79999").await.unwrap();

    // 有効な認証情報でのテスト
    let valid_client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:79999".parse().unwrap(),
        auth: AuthConfig {
            token: "valid_token".to_string(),
            private_key: load_test_private_key(),
        },
        ..Default::default()
    });

    assert!(valid_client.connect().await.is_ok());

    // 無効な認証情報でのテスト
    let invalid_client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:79999".parse().unwrap(),
        auth: AuthConfig {
            token: "invalid_token".to_string(),
            private_key: load_test_private_key(),
        },
        ..Default::default()
    });

    assert!(invalid_client.connect().await.is_err());

    // 認証なしでのテスト
    let no_auth_client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:79999".parse().unwrap(),
        auth: AuthConfig::default(),
        ..Default::default()
    });

    assert!(no_auth_client.connect().await.is_err());
}

```

**暗号化強度テスト**:

```rust
#[tokio::test]
async fn test_encryption_strength() {
    let router = SecureTestRouter::start("127.0.0.1:89999").await.unwrap();
    let target = TestServer::start("127.0.0.1:88080").await.unwrap();

    let client = ConduitClient::new(ClientConfig {
        router_addr: "127.0.0.1:89999".parse().unwrap(),
        target_addr: "127.0.0.1:88080".parse().unwrap(),
        bind_addr: "127.0.0.1:87070".parse().unwrap(),
        tls_config: TlsConfig {
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
            ],
            min_version: "1.3".to_string(),
        },
        ..Default::default()
    });

    client.start().await.unwrap();

    // 暗号化されたトラフィックの検証
    let connection_info = client.get_connection_info().await.unwrap();
    assert_eq!(connection_info.tls_version, "1.3");
    assert!(connection_info.cipher_suite.contains("AES_256_GCM") ||
             connection_info.cipher_suite.contains("CHACHA20_POLY1305"));

    // 中間者攻撃テスト
    let mitm_result = simulate_mitm_attack("127.0.0.1:87070").await;
    assert!(mitm_result.is_err()); // 攻撃が失敗することを確認

    client.shutdown().await.unwrap();
}

```

**入力検証テスト**:

```rust
#[tokio::test]
async fn test_input_validation() {
    let router = TestRouter::start("127.0.0.1:99999").await.unwrap();

    // 異常なサイズのデータ
    let oversized_data = vec![0xFF; 1024 * 1024 * 100]; // 100MB
    let result = send_malformed_data("127.0.0.1:99999", &oversized_data).await;
    assert!(result.is_err());

    // 不正なプロトコルメッセージ
    let malformed_message = b"\\xFF\\xFF\\xFF\\xFF invalid message";
    let result = send_malformed_data("127.0.0.1:99999", malformed_message).await;
    assert!(result.is_err());

    // SQL インジェクション風の文字列
    let injection_attempt = b"'; DROP TABLE users; --";
    let result = send_malformed_data("127.0.0.1:99999", injection_attempt).await;
    assert!(result.is_err());

    router.shutdown().await.unwrap();
}

```

### CI/CD パイプライン

### GitHub Actions ワークフロー

```yaml
# .github/workflows/ci.yml
name: Conduit CI/CD Pipeline

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]
  release:
    types: [published]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Test Suite
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
        rust: [stable, beta]
        include:
          - os: ubuntu-latest
            rust: nightly

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: ${{ matrix.rust }}
          override: true
          components: rustfmt, clippy

      - name: Cache dependencies
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Run unit tests
        run: cargo test --lib --all-features

      - name: Run integration tests
        run: cargo test --test integration --all-features

      - name: Run performance tests
        if: matrix.os == 'ubuntu-latest' && matrix.rust == 'stable'
        run: cargo test --test performance --all-features --release

      - name: Generate test coverage
        if: matrix.os == 'ubuntu-latest' && matrix.rust == 'stable'
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --all-features --workspace --timeout 120 --out Xml

      - name: Upload coverage to Codecov
        if: matrix.os == 'ubuntu-latest' && matrix.rust == 'stable'
        uses: codecov/codecov-action@v3
        with:
          file: cobertura.xml

  security:
    name: Security Audit
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          override: true

      - name: Security audit
        uses: actions-rs/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Run security tests
        run: cargo test --test security --all-features

  docker:
    name: Docker Build & Test
    runs-on: ubuntu-latest
    needs: [test, security]

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2

      - name: Build Docker image
        uses: docker/build-push-action@v4
        with:
          context: .
          push: false
          tags: conduit:test
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Test Docker image
        run: |
          docker run --rm conduit:test conduit --version
          docker run --rm conduit:test conduit --help

  benchmark:
    name: Performance Benchmark
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          override: true

      - name: Run benchmarks
        run: |
          cargo install cargo-criterion
          cargo criterion --output-format verbose > benchmark_results.txt

      - name: Upload benchmark results
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-results
          path: benchmark_results.txt

  release:
    name: Release Build
    runs-on: ${{ matrix.os }}
    if: github.event_name == 'release'
    needs: [test, security, docker]

    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact_name: conduit
            asset_name: conduit-linux-amd64
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact_name: conduit
            asset_name: conduit-linux-arm64
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact_name: conduit.exe
            asset_name: conduit-windows-amd64.exe
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact_name: conduit
            asset_name: conduit-macos-intel
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact_name: conduit
            asset_name: conduit-macos-apple-silicon

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          target: ${{ matrix.target }}
          override: true

      - name: Build release binary
        uses: actions-rs/cargo@v1
        with:
          command: build
          args: --release --target ${{ matrix.target }}

      - name: Upload release asset
        uses: actions/upload-release-asset@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          upload_url: ${{ github.event.release.upload_url }}
          asset_path: ./target/${{ matrix.target }}/release/${{ matrix.artifact_name }}
          asset_name: ${{ matrix.asset_name }}
          asset_content_type: application/octet-stream

  docker-release:
    name: Docker Release
    runs-on: ubuntu-latest
    if: github.event_name == 'release'
    needs: [release]

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2

      - name: Login to Docker Hub
        uses: docker/login-action@v2
        with:
          username: ${{ secrets.DOCKER_USERNAME }}
          password: ${{ secrets.DOCKER_PASSWORD }}

      - name: Login to GitHub Container Registry
        uses: docker/login-action@v2
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v4
        with:
          images: |
            conduit/conduit
            ghcr.io/${{ github.repository }}
          tags: |
            type=ref,event=branch
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=semver,pattern={{major}}

      - name: Build and push Docker images
        uses: docker/build-push-action@v4
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
```

### 品質ゲート

```yaml
# .github/workflows/quality-gate.yml
name: Quality Gate

on:
  pull_request:
    branches: [main]

jobs:
  quality-check:
    name: Quality Gate Checks
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          override: true
          components: rustfmt, clippy

      - name: Check code coverage
        run: |
          cargo install cargo-tarpaulin
          coverage=$(cargo tarpaulin --all-features --workspace --timeout 120 --output-format json | jq '.files | map(.coverage) | add / length')
          echo "Coverage: $coverage%"
          if (( $(echo "$coverage < 90" | bc -l) )); then
            echo "❌ Code coverage below 90%: $coverage%"
            exit 1
          fi
          echo "✅ Code coverage acceptable: $coverage%"

      - name: Check performance regression
        run: |
          cargo install cargo-criterion
          cargo criterion --message-format json > new_benchmark.json

          # 前回のベンチマーク結果と比較
          if [ -f "benchmark_baseline.json" ]; then
            python3 scripts/compare_benchmarks.py benchmark_baseline.json new_benchmark.json
          fi

      - name: Security vulnerability scan
        run: |
          cargo audit
          cargo deny check

      - name: Documentation check
        run: |
          cargo doc --all-features --no-deps
          echo "✅ Documentation builds successfully"

      - name: License compliance check
        run: |
          cargo install cargo-license
          cargo license --json | jq '.[] | select(.license | test("GPL|AGPL"; "i")) | length' > gpl_count.txt
          if [ $(cat gpl_count.txt) -gt 0 ]; then
            echo "❌ GPL licensed dependencies found"
            exit 1
          fi
          echo "✅ License compliance verified"
```

### テスト実行環境

### テストヘルパー

```rust
// tests/common/mod.rs
use std::net::SocketAddr;
use std::sync::Once;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static INIT: Once = Once::new();

pub fn init_test_environment() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

pub struct TestRouter {
    addr: SocketAddr,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl TestRouter {
    pub async fn start(bind_addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        if let Ok((stream, _)) = result {
                            tokio::spawn(handle_test_client(stream));
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            addr,
            shutdown_tx,
            handle,
        })
    }

    pub async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        self.handle.await?;
        Ok(())
    }
}

async fn handle_test_client(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0; 1024];

    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            break;
        }

        // エコーサーバーとして動作
        stream.write_all(&buffer[..n]).await?;
    }

    Ok(())
}

pub struct TestServer {
    addr: SocketAddr,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    pub async fn start(bind_addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(bind_addr).await?;
        let addr = listener.local_addr()?;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        if let Ok((stream, _)) = result {
                            tokio::spawn(handle_http_request(stream));
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            addr,
            shutdown_tx,
            handle,
        })
    }

    pub async fn shutdown(self) -> Result<()> {
        let _ = self.shutdown_tx.send(());
        self.handle.await?;
        Ok(())
    }
}

async fn handle_http_request(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0; 1024];
    let _n = stream.read(&mut buffer).await?;

    let response = "HTTP/1.1 200 OK\\r\\nContent-Length: 13\\r\\n\\r\\nHello, World!";
    stream.write_all(response.as_bytes()).await?;

    Ok(())
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

```

### テスト実行コマンド

```bash
# 全テスト実行
cargo test --all-features

# 単体テストのみ
cargo test --lib

# 統合テストのみ
cargo test --test integration

# パフォーマンステストのみ
cargo test --test performance --release

# セキュリティテストのみ
cargo test --test security

# カバレッジ測定
cargo tarpaulin --all-features --workspace --timeout 120

# ベンチマーク実行
cargo criterion

# 並列テスト無効化（ネットワークテスト用）
cargo test -- --test-threads=1

```

## 実装ガイドライン

### 開発環境

**必要要件**:

- Rust 1.70+ (Edition 2021)
- [`rustfmt`](dev-tools.md), [`clippy`](dev-tools.md) コンポーネント
- [Git](dev-tools.md), [Docker](dev-tools.md) (オプション)

**プロジェクト構造**:

```
conduit/
├── src/
│   ├── cli/          # コマンドライン処理
│   ├── tunnel/       # トンネル機能
│   ├── security/     # 暗号化・認証
│   └── config/       # 設定管理
├── tests/            # テストコード
└── docs/             # ドキュメント
```

### 実装フェーズ

**Phase 1: コア機能**（4週間）

- CLI基盤
- TLS通信
- 基本トンネリング

**Phase 2: 運用機能**（3週間）

- 設定管理
- ログ・監視
- エラーハンドリング

**Phase 3: 拡張機能**（4週間）

- API統合
- ヘルスチェック
- パフォーマンス最適化

詳細な実装手順は [`docs/implementation-guide.md`](implementation-guide.md) を参照。
│ │ │ ├── mod.rs
│ │ │ ├── init.rs # conduit init
│ │ │ ├── start.rs # conduit start
│ │ │ ├── up.rs # conduit up
│ │ │ ├── down.rs # conduit down
│ │ │ ├── router.rs # conduit router
│ │ │ ├── list.rs # conduit list
│ │ │ ├── kill.rs # conduit kill
│ │ │ ├── stats.rs # conduit stats
│ │ │ ├── config.rs # conduit config
│ │ │ ├── logs.rs # conduit logs
│ │ │ ├── health.rs # conduit health
│ │ │ ├── profile.rs # conduit profile
│ │ │ ├── api.rs # conduit api
│ │ │ └── keys.rs # conduit keys
│ │ ├── args.rs # 引数定義
│ │ └── output.rs # 出力フォーマット
│ ├── client/ # クライアント実装
│ │ ├── mod.rs
│ │ ├── tunnel.rs # トンネル管理
│ │ ├── connection.rs # 接続管理
│ │ ├── manager.rs # クライアントマネージャー
│ │ └── reconnect.rs # 再接続ロジック
│ ├── router/ # ルーター実装
│ │ ├── mod.rs
│ │ ├── server.rs # サーバー実装
│ │ ├── client_handler.rs # クライアント処理
│ │ ├── tunnel_registry.rs # トンネル登録管理
│ │ └── load_balancer.rs # ロードバランサー
│ ├── security/ # セキュリティ
│ │ ├── mod.rs
│ │ ├── tls.rs # TLS設定
│ │ ├── keys.rs # キー管理
│ │ ├── auth.rs # 認証
│ │ ├── rotation.rs # キーローテーション
│ │ └── validator.rs # 入力検証
│ ├── config/ # 設定管理
│ │ ├── mod.rs
│ │ ├── loader.rs # 設定読み込み
│ │ ├── validation.rs # 設定検証
│ │ ├── profile.rs # プロファイル管理
│ │ ├── env.rs # 環境変数処理
│ │ └── defaults.rs # デフォルト値
│ ├── protocol/ # プロトコル
│ │ ├── mod.rs
│ │ ├── messages.rs # メッセージ定義
│ │ ├── framing.rs # フレーミング
│ │ ├── handshake.rs # ハンドシェイク
│ │ └── codec.rs # エンコーディング
│ ├── monitoring/ # 監視
│ │ ├── mod.rs
│ │ ├── metrics.rs # メトリクス
│ │ ├── health.rs # ヘルスチェック
│ │ ├── alerts.rs # アラート
│ │ ├── tracing.rs # トレーシング
│ │ └── dashboard.rs # ダッシュボード
│ ├── api/ # REST API
│ │ ├── mod.rs
│ │ ├── handlers/ # APIハンドラー
│ │ │ ├── mod.rs
│ │ │ ├── tunnels.rs
│ │ │ ├── connections.rs
│ │ │ ├── metrics.rs
│ │ │ ├── config.rs
│ │ │ └── health.rs
│ │ ├── middleware.rs # ミドルウェア
│ │ ├── auth.rs # API認証
│ │ └── response.rs # レスポンス型
│ ├── webhook/ # Webhook
│ │ ├── mod.rs
│ │ ├── manager.rs # Webhook管理
│ │ ├── events.rs # イベント定義
│ │ └── sender.rs # 送信処理
│ ├── storage/ # ストレージ
│ │ ├── mod.rs
│ │ ├── memory.rs # インメモリストレージ
│ │ ├── file.rs # ファイルストレージ
│ │ └── backup.rs # バックアップ
│ ├── network/ # ネットワーク
│ │ ├── mod.rs
│ │ ├── tcp.rs # TCP処理
│ │ ├── udp.rs # UDP処理
│ │ ├── proxy.rs # プロキシ
│ │ └── bandwidth.rs # 帯域制限
│ └── utils/ # ユーティリティ
│ ├── mod.rs
│ ├── errors.rs # エラー定義
│ ├── retry.rs # リトライロジック
│ ├── time.rs # 時間処理
│ ├── crypto.rs # 暗号化ユーティリティ
│ ├── format.rs # フォーマット
│ └── fs.rs # ファイルシステム
├── tests/ # テスト
│ ├── common/ # テスト共通
│ │ ├── mod.rs
│ │ ├── fixtures.rs # テストデータ
│ │ ├── helpers.rs # テストヘルパー
│ │ └── mock.rs # モック
│ ├── integration/ # 統合テスト
│ │ ├── basic_tunnel.rs
│ │ ├── multi_tunnel.rs
│ │ ├── security.rs
│ │ ├── failover.rs
│ │ └── api.rs
│ ├── performance/ # パフォーマンステスト
│ │ ├── throughput.rs
│ │ ├── latency.rs
│ │ ├── memory.rs
│ │ └── scalability.rs
│ └── security/ # セキュリティテスト
│ ├── auth.rs
│ ├── encryption.rs
│ ├── input_validation.rs
│ └── penetration.rs
├── benches/ # ベンチマーク
│ ├── tunnel_throughput.rs
│ ├── crypto_performance.rs
│ └── protocol_overhead.rs
├── examples/ # 使用例
│ ├── basic_usage.rs
│ ├── advanced_config.rs
│ ├── custom_auth.rs
│ └── kubernetes_deploy.rs
├── docs/ # ドキュメント
│ ├── README.md
│ ├── ARCHITECTURE.md
│ ├── API.md
│ ├── DEPLOYMENT.md
│ ├── SECURITY.md
│ ├── TROUBLESHOOTING.md
│ └── images/
├── scripts/ # スクリプト
│ ├── setup-dev-env.sh
│ ├── build-release.sh
│ ├── run-tests.sh
│ ├── generate-certs.sh
│ └── benchmark.sh
├── docker/ # Docker関連
│ ├── Dockerfile
│ ├── Dockerfile.alpine
│ ├── docker-compose.yml
│ ├── docker-compose.dev.yml
│ └── kubernetes/
│ ├── deployment.yaml
│ ├── service.yaml
│ ├── configmap.yaml
│ ├── secret.yaml
│ └── ingress.yaml
├── config/ # 設定例
│ ├── conduit.toml
│ ├── conduit.dev.toml
│ ├── conduit.prod.toml
│ └── profiles/
│ ├── development.toml
│ ├── staging.toml
│ └── production.toml
└── .github/ # GitHub設定
├── workflows/
│ ├── ci.yml
│ ├── quality-gate.yml
│ ├── security.yml
│ └── release.yml
├── ISSUE_TEMPLATE/
│ ├── bug_report.md
│ ├── feature_request.md
│ └── security.md
├── PULL_REQUEST_TEMPLATE.md
└── dependabot.yml

````

### Cargo.toml設定

```toml
# Cargo.toml
[package]
name = "conduit"
version = "2.0.0"
edition = "2021"
rust-version = "1.70"
authors = ["HidemaruOwO <hideмарuo@example.com>"]
description = "High-performance network tunneling software"
documentation = "<https://docs.rs/conduit>"
homepage = "<https://github.com/HidemaruOwO/conduit>"
repository = "<https://github.com/HidemaruOwO/conduit>"
license = "Apache-2.0"
keywords = ["tunnel", "network", "proxy", "security", "tls"]
categories = ["network-programming", "command-line-utilities"]
readme = "README.md"
exclude = [
    "/.github/",
    "/docs/",
    "/scripts/",
    "/benches/",
    "/examples/",
    "*.log"
]

[dependencies]
# 非同期ランタイム
tokio = { version = "1.35", features = ["full"] }
tokio-rustls = "0.24"

# TLS・暗号化
rustls = { version = "0.21", features = ["dangerous_configuration"] }
ed25519-dalek = { version = "2.0", features = ["rand_core"] }
ring = "0.17"
rand = "0.8"

# CLI
clap = { version = "4.4", features = ["derive", "color", "suggestions"] }

# 設定・シリアライゼーション
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
serde_json = "1.0"
bincode = "1.3"

# エラーハンドリング
anyhow = "1.0"
thiserror = "1.0"

# ログ・トレーシング
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# ユーティリティ
uuid = { version = "1.6", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
dashmap = "5.5"
base64 = "0.21"

# ネットワーク・HTTP
reqwest = { version = "0.11", features = ["json", "rustls-tls"], default-features = false }
axum = { version = "0.7", features = ["ws", "macros"] }
tower = { version = "0.4", features = ["timeout", "limit"] }
tower-http = { version = "0.5", features = ["cors", "compression-gzip"] }

# メトリクス・監視
prometheus = { version = "0.13", features = ["process"] }
sysinfo = "0.29"

[dev-dependencies]
tokio-test = "0.4"
mockall = "0.12"
tempfile = "3.8"
criterion = { version = "0.5", features = ["html_reports"] }
proptest = "1.4"
wiremock = "0.5"

[build-dependencies]
vergen = { version = "8.2", features = ["build", "git", "gitcl", "cargo"] }

[[bin]]
name = "conduit"
path = "src/main.rs"

[lib]
name = "conduit"
path = "src/lib.rs"

[[bench]]
name = "tunnel_throughput"
harness = false

[[bench]]
name = "crypto_performance"
harness = false

[[bench]]
name = "protocol_overhead"
harness = false

[profile.dev]
opt-level = 0
debug = true
split-debuginfo = "unpacked"
debug-assertions = true
overflow-checks = true
lto = false
panic = "unwind"
incremental = true
codegen-units = 256

[profile.release]
opt-level = 3
debug = false
debug-assertions = false
overflow-checks = false
lto = "thin"
panic = "abort"
incremental = false
codegen-units = 1
strip = true

[profile.bench]
inherits = "release"
debug = true
strip = false

[profile.test]
opt-level = 1
debug = true
debug-assertions = true
overflow-checks = true

# カーゴの機能フラグ
[features]
default = ["api", "metrics", "webhooks"]
api = ["axum", "tower", "tower-http"]
metrics = ["prometheus", "sysinfo"]
webhooks = ["reqwest"]
docker = []
kubernetes = []
development = ["tokio-test", "mockall"]

# ワークスペース設定（将来の拡張用）
[workspace]
members = [
    ".",
    "crates/*"
]
resolver = "2"

# cargo-deny設定
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]

# リリース設定
[package.metadata.release]
pre-release-replacements = [
    { file = "CHANGELOG.md", search = "Unreleased", replace = "{{version}}" },
    { file = "README.md", search = "conduit-v[0-9\\\\.]+", replace = "conduit-v{{version}}" },
]

````

### コーディング規約

### Rustコーディングスタイル

**命名規約**:

```rust
// モジュール: snake_case
mod tunnel_manager;
mod config_loader;

// 構造体・列挙型: PascalCase
struct TunnelConfig;
enum ConnectionState;

// 関数・変数: snake_case
fn create_tunnel() -> Result<()>;
let connection_count = 0;

// 定数: SCREAMING_SNAKE_CASE
const MAX_CONNECTIONS: u32 = 10000;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

// トレイト: PascalCase
trait TunnelManager;
trait ConnectionHandler;

```

**エラーハンドリング規約**:

```rust
// ✅ 良い例
fn parse_config(content: &str) -> Result<Config, ConduitError> {
    let config: Config = toml::from_str(content)
        .map_err(|e| ConduitError::ConfigError {
            message: format!("Failed to parse TOML: {}", e)
        })?;

    validate_config(&config)?;
    Ok(config)
}

// ❌ 悪い例
fn parse_config(content: &str) -> Config {
    toml::from_str(content).unwrap() // パニックの可能性
}

// 関数内でのエラー処理
async fn connect_to_router(&self) -> Result<TlsStream<TcpStream>> {
    let stream = TcpStream::connect(&self.router_addr)
        .await
        .map_err(|e| ConduitError::NetworkError { source: e })?;

    let connector = TlsConnector::from(self.tls_config.clone());
    let domain = rustls::ServerName::try_from(self.router_host.as_str())
        .map_err(|e| ConduitError::TlsError {
            message: format!("Invalid server name: {}", e)
        })?;

    let tls_stream = connector.connect(domain, stream)
        .await
        .map_err(|e| ConduitError::TlsError {
            message: format!("TLS handshake failed: {}", e)
        })?;

    Ok(tls_stream)
}

```

**ログ・トレーシング規約**:

```rust
use tracing::{info, warn, error, debug, instrument};

// ✅ 構造化ログ
#[instrument(skip(stream), fields(client_addr = %addr))]
async fn handle_client_connection(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    info!("New client connection established");

    match process_connection(stream).await {
        Ok(()) => {
            info!("Connection processed successfully");
        }
        Err(e) => {
            error!(error = %e, "Failed to process connection");
            return Err(e);
        }
    }

    Ok(())
}

// トレーシングスパンの使用
async fn tunnel_data(source: &mut TcpStream, target: &mut TcpStream) -> Result<u64> {
    let span = tracing::info_span!("tunnel_data");
    let _enter = span.enter();

    let mut buffer = [0; 8192];
    let mut total_bytes = 0u64;

    loop {
        let n = source.read(&mut buffer).await?;
        if n == 0 {
            debug!("Source stream closed");
            break;
        }

        target.write_all(&buffer[..n]).await?;
        total_bytes += n as u64;

        debug!(bytes_transferred = n, total_bytes, "Data tunneled");
    }

    info!(total_bytes, "Tunnel completed");
    Ok(total_bytes)
}

```

**非同期処理規約**:

```rust
// ✅ 適切なタスク分離
struct TunnelManager {
    active_tunnels: Arc<DashMap<String, TunnelHandle>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl TunnelManager {
    async fn start_tunnel(&self, config: TunnelConfig) -> Result<String> {
        let tunnel_id = Uuid::new_v4().to_string();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let tunnel = Tunnel::new(config);

            tokio::select! {
                result = tunnel.run() => {
                    if let Err(e) = result {
                        error!(tunnel_id = %tunnel_id, error = %e, "Tunnel failed");
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!(tunnel_id = %tunnel_id, "Tunnel shutdown requested");
                }
            }
        });

        self.active_tunnels.insert(tunnel_id.clone(), TunnelHandle { handle });
        Ok(tunnel_id)
    }
}

// ✅ グレースフルシャットダウン
async fn graceful_shutdown(tunnels: Arc<DashMap<String, TunnelHandle>>) {
    info!("Starting graceful shutdown");

    let shutdown_futures: Vec<_> = tunnels
        .iter()
        .map(|entry| {
            let handle = entry.value().handle.clone();
            async move {
                if let Err(e) = handle.await {
                    warn!(error = %e, "Tunnel shutdown error");
                }
            }
        })
        .collect();

    // 全てのトンネルが終了するまで最大30秒待機
    tokio::time::timeout(Duration::from_secs(30), futures::future::join_all(shutdown_futures))
        .await
        .unwrap_or_else(|_| {
            warn!("Graceful shutdown timeout, forcing exit");
        });

    info!("Graceful shutdown completed");
}

```

**テスト規約**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    // ✅ テスト関数命名: test_<機能>_<条件>_<期待結果>
    #[tokio::test]
    async fn test_tunnel_creation_with_valid_config_succeeds() {
        let config = TunnelConfig {
            name: "test-tunnel".to_string(),
            target_addr: "127.0.0.1:8080".parse().unwrap(),
            bind_addr: "127.0.0.1:9090".parse().unwrap(),
            protocol: Protocol::Tcp,
            max_connections: 100,
            timeout: Duration::from_secs(30),
        };

        let result = TunnelManager::create_tunnel(config).await;
        assert!(result.is_ok());

        let tunnel = result.unwrap();
        assert_eq!(tunnel.config.name, "test-tunnel");
        assert_eq!(tunnel.state, TunnelState::Created);
    }

    #[tokio::test]
    async fn test_tunnel_creation_with_invalid_address_fails() {
        let config = TunnelConfig {
            target_addr: "invalid:address".parse().expect_err("Should fail to parse"),
            ..Default::default()
        };

        // この例では、実際にはparse()の段階でエラーになるため、
        // 適切なテストケースに調整が必要
    }

    // ✅ プロパティベーステスト
    #[tokio::test]
    async fn test_config_roundtrip_serialization() {
        use proptest::prelude::*;

        proptest!(|(config in any::<TunnelConfig>())| {
            let serialized = toml::to_string(&config).unwrap();
            let deserialized: TunnelConfig = toml::from_str(&serialized).unwrap();
            assert_eq!(config, deserialized);
        });
    }
}

```

### デーモンレスアーキテクチャ実装計画

### Phase 1: gRPC Infrastructure (P0) - 2週間

**Week 1: gRPCプロトコル定義とコード生成**

```protobuf
// src/grpc/tunnel.proto
syntax = "proto3";

package tunnel;

service TunnelControl {
  rpc GetStatus(StatusRequest) returns (StatusResponse);
  rpc ListConnections(ListRequest) returns (ListResponse);
  rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
  rpc GetMetrics(MetricsRequest) returns (MetricsResponse);
}

message TunnelInfo {
  string id = 1;
  string name = 2;
  string router_addr = 3;
  string source_addr = 4;
  string bind_addr = 5;
  string status = 6;
  int64 created_at = 7;
  int32 active_connections = 8;
}

message StatusResponse {
  TunnelInfo tunnel = 1;
  repeated ConnectionInfo connections = 2;
  TunnelMetrics metrics = 3;
}
```

```rust
// src/grpc/server.rs - gRPCサーバー実装
#[tonic::async_trait]
impl TunnelControl for TunnelControlService {
    async fn get_status(&self, _: Request<StatusRequest>)
        -> Result<Response<StatusResponse>, Status> {
        let tunnel_info = self.tunnel_manager.get_tunnel_info().await;
        let connections = self.tunnel_manager.list_connections().await;
        let metrics = self.tunnel_manager.get_metrics().await;
        
        Ok(Response::new(StatusResponse {
            tunnel: Some(tunnel_info),
            connections,
            metrics: Some(metrics),
        }))
    }
}

// src/grpc/client.rs - gRPCクライアント実装
pub struct TunnelControlClient {
    inner: TunnelControlClientInner,
}

impl TunnelControlClient {
    pub async fn connect(addr: String) -> Result<Self, Box<dyn std::error::Error>> {
        let inner = TunnelControlClientInner::connect(addr).await?;
        Ok(Self { inner })
    }
    
    pub async fn get_status(&mut self) -> Result<StatusResponse, Box<dyn std::error::Error>> {
        let response = self.inner.get_status(StatusRequest {}).await?;
        Ok(response.into_inner())
    }
}
```

**Week 2: Process Registry実装**

```rust
// src/registry/manager.rs
pub struct ProcessRegistry {
    registry_dir: PathBuf,
}

impl ProcessRegistry {
    pub fn new() -> Result<Self, RegistryError> {
        let home_dir = dirs::home_dir().ok_or(RegistryError::HomeDirectoryNotFound)?;
        let registry_dir = home_dir.join(".conduit/tunnels");
        std::fs::create_dir_all(&registry_dir)?;
        
        Ok(Self { registry_dir })
    }
    
    pub fn register_process(&self, info: ProcessInfo) -> Result<(), RegistryError> {
        let file_path = self.registry_dir.join(format!("{}.json", info.tunnel_id));
        let json = serde_json::to_string_pretty(&info)?;
        std::fs::write(file_path, json)?;
        Ok(())
    }
    
    pub fn list_active_processes(&self) -> Result<Vec<ProcessInfo>, RegistryError> {
        let mut processes = Vec::new();
        
        for entry in std::fs::read_dir(&self.registry_dir)? {
            let entry = entry?;
            if entry.path().extension() == Some(std::ffi::OsStr::new("json")) {
                let content = std::fs::read_to_string(entry.path())?;
                let info: ProcessInfo = serde_json::from_str(&content)?;
                
                // プロセスが実際に動作しているかチェック
                if self.is_process_alive(info.pid) {
                    processes.push(info);
                } else {
                    // 死んだプロセスのファイルを削除
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        
        Ok(processes)
    }
}
```

### Phase 2: Command Implementation (P0) - 2週間

**Week 3: CLI Commands修正**

```rust
// src/cli/commands/start.rs - バックグラウンドプロセス起動
pub async fn execute(args: StartArgs) -> CommandResult {
    let config = TunnelConfig::from_args(&args);
    
    // サービスファイル生成モード
    if let Some(service_type) = args.service_file {
        return generate_service_file(&config, &service_type);
    }
    
    // 通常のトンネル起動モード
    let registry = ProcessRegistry::new()?;
    let grpc_port = find_available_port()?;
    
    // バックグラウンドでトンネルプロセスを起動
    let child = Command::new(std::env::current_exe()?)
        .args(&["_tunnel_process"])
        .env("TUNNEL_CONFIG", serde_json::to_string(&config)?)
        .env("GRPC_PORT", grpc_port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    
    // Process Registryに登録
    let process_info = ProcessInfo {
        tunnel_id: config.id.clone(),
        name: config.name.clone(),
        pid: child.id(),
        grpc_port,
        config: config.clone(),
        status: ProcessStatus::Starting,
        created_at: Utc::now(),
    };
    
    registry.register_process(process_info)?;
    
    println!("🚀 Tunnel started: {} (PID: {})", config.name, child.id());
    Ok(())
}

// src/cli/commands/list.rs - gRPC経由でプロセス情報取得
pub async fn execute(args: ListArgs) -> CommandResult {
    let registry = ProcessRegistry::new()?;
    let processes = registry.list_active_processes()?;
    
    if processes.is_empty() {
        println!("No active tunnels found");
        return Ok(());
    }
    
    println!("📋 Active Tunnels:");
    for process in processes {
        let mut client = TunnelControlClient::connect(
            format!("http://127.0.0.1:{}", process.grpc_port)
        ).await?;
        
        match client.get_status().await {
            Ok(status) => {
                if args.format == OutputFormat::Json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    print_tunnel_status(&status, &args);
                }
            }
            Err(e) => {
                eprintln!("Failed to get status for {}: {}", process.name, e);
            }
        }
    }
    
    Ok(())
}
```

**Week 4: Service File Generation**

```rust
// src/service/generator.rs
pub fn generate_service_file(config: &TunnelConfig, service_type: &str) -> Result<String, ServiceError> {
    match service_type {
        "systemd" => generate_systemd_service(config),
        "launchd" => generate_launchd_service(config),
        "openrc" => generate_openrc_service(config),
        "rc.d" => generate_rcd_service(config),
        _ => Err(ServiceError::UnsupportedServiceType(service_type.to_string())),
    }
}

fn generate_systemd_service(config: &TunnelConfig) -> Result<String, ServiceError> {
    let template = r#"[Unit]
Description=Conduit Tunnel - {name}
After=network.target
Wants=network.target

[Service]
Type=exec
ExecStart={binary_path} start --router {router} --source {source} --bind {bind}
Restart=always
RestartSec=5
User={user}
Group={group}

[Install]
WantedBy=multi-user.target
"#;

    let service_content = template
        .replace("{name}", &config.name)
        .replace("{binary_path}", &std::env::current_exe()?.to_string_lossy())
        .replace("{router}", &config.router_addr.to_string())
        .replace("{source}", &config.source_addr.to_string())
        .replace("{bind}", &config.bind_addr.to_string())
        .replace("{user}", &get_current_user()?)
        .replace("{group}", &get_current_group()?);
    
    Ok(service_content)
}
```

### Phase 3: Integration & Testing (P0) - 1週間

**Week 5: 統合テスト・品質保証**

```rust
// tests/integration_tests.rs
#[tokio::test]
async fn test_daemonless_architecture_flow() {
    // 1. conduit start でトンネルプロセス起動
    let output = Command::new("target/debug/conduit")
        .args(&["start", "--router", "127.0.0.1:9999", "--source", "127.0.0.1:8080", "--bind", "127.0.0.1:8081"])
        .output()
        .await
        .expect("Failed to execute conduit start");
    
    assert!(output.status.success());
    
    // 2. conduit list でプロセス確認
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    let output = Command::new("target/debug/conduit")
        .args(&["list"])
        .output()
        .await
        .expect("Failed to execute conduit list");
    
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Active Tunnels"));
    
    // 3. conduit kill でプロセス終了
    let output = Command::new("target/debug/conduit")
        .args(&["kill", "--all"])
        .output()
        .await
        .expect("Failed to execute conduit kill");
    
    assert!(output.status.success());
}
```

### 依存関係追加

```toml
# Cargo.tomlに追加
[dependencies]
# 既存の依存関係...
tonic = "0.10"
prost = "0.12"
prost-types = "0.12"
dirs = "5.0"

[build-dependencies]
tonic-build = "0.10"
```

```

**Week 2: セキュリティ実装**

```rust
// Day 8-10: TLS設定とEd25519キー管理
// src/security/tls.rs
pub fn create_client_tls_config(
    cert_path: &Path,
    key_path: &Path,
    ca_path: Option<&Path>
) -> Result<Arc<ClientConfig>> {
    // TLS設定実装
}

// src/security/keys.rs - 前回定義したKeyManager実装

// Day 11-14: 認証システム
// src/security/auth.rs
pub struct AuthManager {
    private_key: ed25519_dalek::Keypair,
    router_token: String,
}

impl AuthManager {
    pub fn sign_message(&self, message: &[u8]) -> Signature {
        self.private_key.sign(message)
    }

    pub fn create_auth_request(&self) -> AuthRequest {
        let timestamp = chrono::Utc::now().timestamp();
        let message = format!("{}:{}", self.router_token, timestamp);
        let signature = self.sign_message(message.as_bytes());

        AuthRequest {
            token: self.router_token.clone(),
            signature: base64::encode(signature.to_bytes()),
        }
    }
}

```

**Week 3: コア通信実装**

```rust
// Day 15-17: プロトコル実装
// src/protocol/messages.rs - 前回定義したTunnelMessage実装
// src/protocol/framing.rs - 前回定義したMessageFramer実装

// Day 18-21: 基本トンネル機能
// src/client/tunnel.rs
pub struct TunnelClient {
    config: TunnelConfig,
    router_connection: Option<Arc<TlsStream<TcpStream>>>,
    active_connections: Arc<DashMap<String, ConnectionInfo>>,
}

impl TunnelClient {
    pub async fn start(&mut self) -> Result<()> {
        self.connect_to_router().await?;
        self.start_local_listener().await?;
        self.handle_connections().await
    }

    async fn connect_to_router(&mut self) -> Result<()> {
        // ルーター接続実装
    }

    async fn start_local_listener(&self) -> Result<()> {
        // ローカルリスナー実装
    }
}

```

**Week 4: ルーター実装**

```rust
// Day 22-25: ルーターサーバー
// src/router/server.rs
pub struct TunnelRouter {
    config: RouterConfig,
    client_manager: Arc<ClientManager>,
    tunnel_registry: Arc<TunnelRegistry>,
}

impl TunnelRouter {
    pub async fn serve(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        info!("Router listening on {}", self.config.bind_addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let handler = ClientHandler::new(
                stream,
                addr,
                self.client_manager.clone(),
                self.tunnel_registry.clone(),
            );

            tokio::spawn(async move {
                if let Err(e) = handler.handle().await {
                    error!(client_addr = %addr, error = %e, "Client handling failed");
                }
            });
        }

        Ok(())
    }
}

// Day 26-28: 統合テストと修正

```

### Phase 2: Essential Features (P0) - 3週間

**Week 5-6: コマンド実装**

```rust
// src/cli/commands/init.rs
pub async fn execute_init(args: InitArgs) -> Result<()> {
    let config_dir = args.config_dir.unwrap_or_else(|| {
        dirs::config_dir().unwrap().join("conduit")
    });

    // ディレクトリ作成
    create_directory_structure(&config_dir).await?;

    // キーペア生成
    generate_keypair(&config_dir).await?;

    // デフォルト設定ファイル作成
    create_default_config(&config_dir).await?;

    println!("✅ Conduit initialized successfully");
    Ok(())
}

// src/cli/commands/start.rs
pub async fn execute_start(args: StartArgs) -> Result<()> {
    let config = TunnelConfig {
        name: args.name.unwrap_or_else(|| generate_tunnel_name()),
        target_addr: args.target.parse()?,
        bind_addr: args.bind.parse()?,
        protocol: args.protocol.unwrap_or(Protocol::Tcp),
        ..Default::default()
    };

    let mut client = TunnelClient::new(config);
    client.start().await
}

// src/cli/commands/up.rs
pub async fn execute_up(args: UpArgs) -> Result<()> {
    let config_path = args.file.unwrap_or_else(|| PathBuf::from("conduit.toml"));
    let conduit_config = ConduitConfig::load_from_file(&config_path)?;

    let manager = TunnelManager::new(conduit_config);
    manager.start_all_tunnels().await
}

```

**Week 7: 監視・メトリクス基盤**

```rust
// src/monitoring/metrics.rs
use prometheus::{Counter, Gauge, Histogram, Registry};

pub struct TunnelMetrics {
    registry: Registry,
    connections_total: Counter,
    active_connections: Gauge,
    bytes_transferred: Counter,
    request_duration: Histogram,
}

impl TunnelMetrics {
    pub fn new() -> Result<Self> {
        let registry = Registry::new();

        let connections_total = Counter::new("conduit_connections_total", "Total connections")?;
        let active_connections = Gauge::new("conduit_active_connections", "Active connections")?;
        let bytes_transferred = Counter::new("conduit_bytes_transferred", "Bytes transferred")?;
        let request_duration = Histogram::new("conduit_request_duration_seconds", "Request duration")?;

        registry.register(Box::new(connections_total.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(bytes_transferred.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;

        Ok(Self {
            registry,
            connections_total,
            active_connections,
            bytes_transferred,
            request_duration,
        })
    }
}

```

### Phase 3: Advanced Features (P1) - 4週間

**Week 8-9: ヘルスチェック・アラート**

```rust
// src/monitoring/health.rs - 前回定義したHealthChecker実装
// src/monitoring/alerts.rs - 前回定義したAlertManager実装

```

**Week 10: API実装**

```rust
// src/api/handlers/tunnels.rs
use axum::{extract::State, Json};

pub async fn list_tunnels(
    State(state): State<ApiState>
) -> Json<ApiResponse<Vec<TunnelInfo>>> {
    let tunnels = state.tunnel_manager.get_all_tunnels().await;
    // 実装
}

pub async fn create_tunnel(
    State(state): State<ApiState>,
    Json(request): Json<CreateTunnelRequest>
) -> Json<ApiResponse<TunnelInfo>> {
    // 実装
}

```

**Week 11: プロファイル管理**

```rust
// src/config/profile.rs - 前回定義したProfileManager実装

```

### Phase 4: Production Features (P2) - 3週間

**Week 12-13: 高度な監視機能**

```rust
// トラフィック解析
// パケットキャプチャ
// パフォーマンス最適化

```

**Week 14: 運用機能**

```rust
// バックアップ・復旧
// 設定バリデーション強化
// デプロイメント支援

```

### 品質保証

### 必須チェック項目

**開発フェーズチェックリスト**:

- [ ] **コード品質**
  - [ ] `cargo fmt --all -- --check` 通過
  - [ ] `cargo clippy --all-targets --all-features -- -D warnings` 通過
  - [ ] 単体テストカバレッジ 90%以上
  - [ ] 統合テスト全通過
  - [ ] パフォーマンステスト目標達成
- [ ] **セキュリティ**
  - [ ] `cargo audit` 脆弱性なし
  - [ ] `cargo deny check` ライセンス・依存関係チェック通過
  - [ ] セキュリティテスト全通過
  - [ ] 静的解析ツール通過
- [ ] **ドキュメント**
  - [ ] API ドキュメント完備
  - [ ] 使用例・チュートリアル作成
  - [ ] アーキテクチャドキュメント更新
  - [ ] トラブルシューティングガイド作成
- [ ] **デプロイメント**
  - [ ] Docker イメージビルド成功
  - [ ] Kubernetes デプロイメント検証
  - [ ] マルチプラットフォームビルド成功
  - [ ] リリースノート作成

### 品質ゲート自動化

```bash
#!/bin/bash
# scripts/quality-gate.sh

set -e

echo "🔍 Running quality gate checks..."

# フォーマットチェック
echo "📝 Checking code formatting..."
cargo fmt --all -- --check

# Clippy
echo "🔧 Running Clippy..."
cargo clippy --all-targets --all-features -- -D warnings

# テスト実行
echo "🧪 Running tests..."
cargo test --all-features

# カバレッジ測定
echo "📊 Measuring test coverage..."
cargo tarpaulin --all-features --workspace --timeout 120 --fail-under 90

# セキュリティ監査
echo "🛡️ Security audit..."
cargo audit
cargo deny check

# ベンチマーク（オプション）
if [ "$RUN_BENCHMARKS" = "true" ]; then
    echo "⚡ Running benchmarks..."
    cargo criterion
fi

# ドキュメント生成
echo "📚 Generating documentation..."
cargo doc --all-features --no-deps

echo "✅ All quality checks passed!"

```

### リリース準備

**リリースチェックリスト**:

- [ ] バージョン番号更新 (Cargo.toml)
- [ ] [CHANGELOG.md](http://changelog.md/) 更新
- [ ] リリースノート作成
- [ ] タグ作成・プッシュ
- [ ] GitHub Release 作成
- [ ] Docker イメージ公開
- [ ] パッケージマネージャー更新通知

**自動リリーススクリプト**:

```bash
#!/bin/bash
# scripts/release.sh

VERSION=$1
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    exit 1
fi

echo "🚀 Preparing release $VERSION..."

# バージョン更新
sed -i "s/^version = .*/version = \\"$VERSION\\"/" Cargo.toml

# Cargo.lock更新
cargo update

# Git コミット・タグ
git add Cargo.toml Cargo.lock
git commit -m "Release v$VERSION"
git tag "v$VERSION"

# ビルドテスト
cargo build --release

# プッシュ
git push origin main
git push origin "v$VERSION"

echo "✅ Release v$VERSION prepared!"
echo "GitHub Actions will handle the rest of the release process."

```

## 付録

### 主要用語

- **Router**: トンネル中継サーバー
- **Client**: トンネル接続クライアント
- **Target**: 転送先サーバー
- **TLS 1.3**: 最新暗号化プロトコル
- **Ed25519**: 高性能デジタル署名

### 参考資料

- **公式ドキュメント**: [Rust](https://doc.rust-lang.org/), [Tokio](https://tokio.rs/), [TLS](https://tools.ietf.org/html/rfc8446)
- **アーキテクチャ参考**: [Tailscale](https://tailscale.com/), [WireGuard](https://www.wireguard.com/)
- **詳細資料**: [`docs/references.md`](references.md)

### ライセンス

**本プロジェクト**: Apache 2.0 + SUSHI-WARE
**主要依存関係**: MIT/Apache 2.0 ライセンス

**Async/Await**

- 非同期プログラミングパターン

**Tokio**

- Rust の非同期ランタイム

### 参考リンク

### 公式ドキュメント・仕様

**Rust エコシステム**

- [Rust Programming Language](https://www.rust-lang.org/) - Rust公式サイト
- [Rust Documentation](https://doc.rust-lang.org/) - Rust言語仕様・標準ライブラリ
- [The Rust Book](https://doc.rust-lang.org/book/) - Rust入門書
- [Rustonomicon](https://doc.rust-lang.org/nomicon/) - アンセーフRustガイド
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) - APIデザインガイドライン

**非同期プログラミング**

- [Tokio Documentation](https://tokio.rs/) - Tokio公式ドキュメント
- [Async Book](https://rust-lang.github.io/async-book/) - Rust非同期プログラミング
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Tokioチュートリアル

**暗号化・セキュリティ**

- [rustls Documentation](https://docs.rs/rustls/) - RustTLS実装
- [RFC 8446 - TLS 1.3](https://tools.ietf.org/html/rfc8446) - TLS 1.3仕様
- [RFC 8032 - Ed25519](https://tools.ietf.org/html/rfc8032) - Ed25519署名仕様
- [ring Cryptography](https://github.com/briansmith/ring) - Rust暗号化ライブラリ

**ネットワーキング**

- [RFC 793 - TCP](https://tools.ietf.org/html/rfc793) - TCP仕様
- [RFC 768 - UDP](https://tools.ietf.org/html/rfc768) - UDP仕様
- [SOCKS Protocol](https://tools.ietf.org/html/rfc1928) - SOCKSプロキシ仕様

### 設計・アーキテクチャ

**分散システム設計**

- [Designing Data-Intensive Applications](https://dataintensive.net/) - Martin Kleppmann
- [Building Microservices](https://www.oreilly.com/library/view/building-microservices/9781491950340/) - Sam Newman
- [Site Reliability Engineering](https://sre.google/books/) - Google SRE Book
- [The Twelve-Factor App](https://12factor.net/) - モダンアプリケーション設計原則

**ネットワークセキュリティ**

- [Applied Cryptography](https://www.schneier.com/books/applied_cryptography/) - Bruce Schneier
- [Serious Cryptography](https://nostarch.com/seriouscrypto) - Jean-Philippe Aumasson
- [Network Security Essentials](https://www.pearson.com/us/higher-education/program/Stallings-Network-Security-Essentials-6th-Edition/PGM334772.html) - William Stallings

### 実装リファレンス

**類似プロジェクト**

- [WireGuard](https://www.wireguard.com/) - 高速VPNプロトコル
- [ngrok](https://ngrok.com/) - セキュアトンネリングサービス
- [Tailscale](https://tailscale.com/) - ゼロコンフィグVPN
- [Bore](https://github.com/ekzhang/bore) - Rust製トンネリングツール
- [rathole](https://github.com/rapiz1/rathole) - Rust製リバースプロキシ

**Rustネットワーキング実装例**

- [Hyper](https://github.com/hyperium/hyper) - HTTP実装
- [Quinn](https://github.com/quinn-rs/quinn) - QUIC実装
- [Tonic](https://github.com/hyperium/tonic) - gRPC実装
- [Warp](https://github.com/seanmonstar/warp) - Webフレームワーク

### 運用・監視

**可観測性**

- [Prometheus Documentation](https://prometheus.io/docs/) - メトリクス監視
- [Jaeger Documentation](https://www.jaegertracing.io/docs/) - 分散トレーシング
- [OpenTelemetry](https://opentelemetry.io/) - 統合可観測性フレームワーク
- [Grafana Documentation](https://grafana.com/docs/) - メトリクス可視化

**コンテナ・オーケストレーション**

- [Docker Documentation](https://docs.docker.com/) - Docker公式ドキュメント
- [Kubernetes Documentation](https://kubernetes.io/docs/) - Kubernetes公式ドキュメント
- [Helm Documentation](https://helm.sh/docs/) - Kubernetesパッケージマネージャー

### 開発ツール・CI/CD

**開発環境**

- [Visual Studio Code](https://code.visualstudio.com/) - エディタ
- [rust-analyzer](https://rust-analyzer.github.io/) - Rust Language Server
- [Clippy](https://github.com/rust-lang/rust-clippy) - Rustリンター

**テスト・品質管理**

- [Criterion.rs](https://github.com/bheisler/criterion.rs) - ベンチマークフレームワーク
- [Proptest](https://github.com/AltSysrq/proptest) - プロパティベーステスト
- [Tarpaulin](https://github.com/xd009642/tarpaulin) - コードカバレッジ
- [Miri](https://github.com/rust-lang/miri) - UBサニタイザー

**CI/CD**

- [GitHub Actions](https://docs.github.com/en/actions) - CI/CDプラットフォーム
- [GitLab CI](https://docs.gitlab.com/ee/ci/) - GitLab統合CI/CD
- [cargo-release](https://github.com/crate-ci/cargo-release) - リリース自動化

### 学習リソース

**Rust学習**

- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - 実例で学ぶRust
- [Rustlings](https://github.com/rust-lang/rustlings) - Rust練習問題
- [Rust Cookbook](https://rust-lang-nursery.github.io/rust-cookbook/) - Rustレシピ集
- [Too Many Lists](https://rust-unofficial.github.io/too-many-lists/) - データ構造実装学習

**ネットワークプログラミング**

- [Beej's Guide to Network Programming](https://beej.us/guide/bgnet/) - ネットワークプログラミング入門
- [TCP/IP Illustrated](https://en.wikipedia.org/wiki/TCP/IP_Illustrated) - TCP/IP詳解
- [Computer Networking: A Top-Down Approach](https://www.pearson.com/us/higher-education/program/Kurose-Computer-Networking-A-Top-Down-Approach-8th-Edition/PGM2877610.html) - ネットワーク教科書

**システムプログラミング**

- [The Linux Programming Interface](https://man7.org/tlpi/) - Linux システムプログラミング
- [Advanced Programming in the UNIX Environment](https://www.pearson.com/us/higher-education/program/Stevens-Advanced-Programming-in-the-UNIX-Environment-3rd-Edition/PGM2746317.html) - UNIX環境プログラミング

### コミュニティ・フォーラム

**Rustコミュニティ**

- [Rust Users Forum](https://users.rust-lang.org/) - ユーザーフォーラム
- [Rust Internals](https://internals.rust-lang.org/) - 言語開発ディスカッション
- [r/rust](https://www.reddit.com/r/rust/) - Reddit Rustコミュニティ
- [Rust Discord](https://discord.gg/rust-lang) - リアルタイムチャット

**技術ディスカッション**

- [Hacker News](https://news.ycombinator.com/) - 技術ニュース
- [Stack Overflow](https://stackoverflow.com/questions/tagged/rust) - 技術Q&A
- [Dev.to](https://dev.to/t/rust) - 開発者ブログプラットフォーム

### ブログ・記事

**Rustネットワーキング**

- [Tokio Blog](https://tokio.rs/blog/) - Tokio開発ブログ
- [Cloudflare Blog - Rust](https://blog.cloudflare.com/tag/rust/) - Cloudflareでのrust活用事例
- [Dropbox Tech Blog - Rust](https://dropbox.tech/infrastructure/rewriting-the-heart-of-our-sync-engine) - 大規模Rust導入事例

**パフォーマンス最適化**

- [The Rust Performance Book](https://nnethercote.github.io/perf-book/) - Rustパフォーマンス最適化
- [Optimizing Rust Performance](https://github.com/nnethercote/perf-book) - パフォーマンス改善手法

### ライセンス情報

### 本プロジェクトのライセンス

**Apache License 2.0**

```
Copyright 2025 HidemaruOwO

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    <http://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

```

**SUSHI-WARE License**

```
HidemaruOwO wrote this software. As long as you retain this notice you
can do whatever you want with this stuff. If we meet some day, and you think
this stuff is worth it, you can buy me sushi in return.

```

### 主要依存関係のライセンス

**コア依存関係**

- `tokio` - MIT License
- `rustls` - Apache-2.0/ISC/MIT
- `ed25519-dalek` - BSD-3-Clause
- `clap` - MIT/Apache-2.0
- `serde` - MIT/Apache-2.0
- `anyhow` - MIT/Apache-2.0
- `thiserror` - MIT/Apache-2.0

**暗号化関連**

- `ring` - ISC/MIT/OpenSSL
- `rand` - MIT/Apache-2.0
- `base64` - MIT/Apache-2.0

**ネットワーク関連**

- `reqwest` - MIT/Apache-2.0
- `axum` - MIT
- `tower` - MIT

**ライセンス互換性**

- すべてのコア依存関係はApache-2.0と互換性あり
- GPLライセンスの依存関係は意図的に排除
- 商用利用に制限なし

### 第三者ライセンス管理

**ライセンスチェック自動化**

```toml
# deny.toml
[licenses]
unlicensed = "deny"
allow = [
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
]
deny = [
    "GPL-2.0",
    "GPL-3.0",
    "AGPL-1.0",
    "AGPL-3.0",
]

[[licenses.exceptions]]
allow = ["OpenSSL"]
name = "ring"

```

**ライセンス監査コマンド**

```bash
# ライセンス一覧表示
cargo license --json

# ライセンス違反チェック
cargo deny check licenses

# 依存関係ツリー表示
cargo tree --format "{p} {l}"

```

### 貢献ガイドライン

### 開発への参加方法

**Issues作成**

- バグレポート: `.github/ISSUE_TEMPLATE/bug_report.md` を使用
- 機能要求: `.github/ISSUE_TEMPLATE/feature_request.md` を使用
- セキュリティ問題: `.github/ISSUE_TEMPLATE/security.md` を使用

**Pull Request**

- フォーク後、featureブランチで開発
- コミットメッセージは [Conventional Commits](https://conventionalcommits.org/) 準拠
- PR作成時は `.github/PULL_REQUEST_TEMPLATE.md` に従い記載
- 全てのCI/CDチェックを通過させること

**コーディング規約**

- 本ドキュメントの「実装ガイドライン」セクションに準拠
- `cargo fmt` と `cargo clippy` を実行してからcommit
- 新機能には必ずテストを追加

### セキュリティ脆弱性報告

**報告先**: [security@hidemaruo.example.com](mailto:security@hidemaruo.example.com)

**報告内容**:

- 脆弱性の詳細説明
- 再現手順
- 影響範囲
- 修正案（あれば）

**対応プロセス**:

1. 24時間以内に受信確認
2. 7日以内に初期分析結果通知
3. 30日以内に修正版リリース（重要度に応じて短縮）
4. 公表は修正版リリース後

### 変更履歴

### Version 1.0.0 (2025-06-11)

**New Features**

- 🎉 初期リリース
- ✨ TLS 1.3 + Ed25519による高セキュリティトンネリング
- ✨ Docker/Kubernetes ネイティブ対応
- ✨ REST API サーバー
- ✨ プロファイル・環境管理
- ✨ 包括的な監視・メトリクス機能
- ✨ Webhook通知システム
- ✨ 自動ヘルスチェック・アラート
- ✨ 鍵ローテーション機能

**Architecture**

- 🏗️ Rust + Tokio非同期アーキテクチャ
- 🏗️ 単一バイナリ設計（クライアント・ルーター統合）
- 🏗️ 階層的設定管理システム
- 🏗️ プラグイン可能な認証システム

**Security**

- 🔒 Ed25519デジタル署名
- 🔒 TLS 1.3暗号化
- 🔒 中央集権型キーローテーション
- 🔒 入力検証・サニタイゼーション

**Performance**

- ⚡ 10,000+同時接続対応
- ⚡ 10Gbps+スループット
- ⚡ <10msレイテンシオーバーヘッド
- ⚡ Zero-copy最適化

**Documentation**

- 📖 完全な技術仕様書
- 📖 API リファレンス
- 📖 デプロイメントガイド
- 📖 トラブルシューティングガイド

---

**Document Metadata**:

- **Version**: 1.0
- **Last Updated**: 2025-06-11 23:02:37 UTC
- **Document Status**: Implementation Ready
- **Specification Status**: Final
- **Maintainer**: HidemaruOwO
- **Next Review Date**: 2025-07-11
- **Language**: Japanese/English
- **Format**: Markdown
- **License**: Apache-2.0 AND SUSHI-WARE

---

**End of Document**
