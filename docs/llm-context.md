 # Conduit プロジェクト - LLM コンテキスト

最終更新: 2025-06-15 03:42 JST
更新者: LLM アシスタント

## プロジェクト概要

### 現在の状況
- **ステータス**: デーモンレスアーキテクチャ設計完成（実装開始可能）
- **プロジェクトタイプ**: Rustで開発される企業級ネットワークトンネリングソフトウェア
- **リポジトリ構造**: MicroRepositoryテンプレートベース
- **主要ドキュメント**: `docs/architecture.md`にデーモンレス設計を含む完全な技術仕様が記載
- **ライセンス**: Apache 2.0 と SUSHI-WARE のデュアルライセンス
- **アーキテクチャ**: Podmanライクなデーモンレス設計に決定

## プロジェクトの理解

### Conduitとは
**外部ユーザーからのアクセスをClient経由でRouter同一サブネット内のサービスに安全かつ高性能に転送**するRust製ネットワークトンネリングソフトウェアです。プライベートネットワーク内のサービスにインターネット経由でアクセスする用途で利用されます。

### 主要な特徴
- **高性能**: Rust + Tokioによる非同期処理
- **セキュア**: Client-Router間をTLS 1.3 + Ed25519で暗号化
- **スケーラブル**: 数万同時接続対応
- **運用性**: Docker/Kubernetesネイティブ対応
- **可観測性**: 包括的な監視・ログ機能
- **単一バイナリ**: ClientとRouter機能を統合

## アーキテクチャ理解

### デーモンレスアーキテクチャ設計（2025-06-15決定）
Conduitは**Podmanライクなデーモンレス設計**を採用し、中央管理デーモンを持たず、独立したトンネルプロセスによる分散管理を実現します。

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
1. **CLI Commands**: ユーザーインターフェース、gRPC経由でTunnel Processと通信
2. **Tunnel Process**: 独立したプロセスとしてトンネルを管理・実行、gRPCサーバーとしても動作
3. **Process Registry**: ファイルベースのTunnel Process情報管理（`~/.conduit/tunnels/`）
4. **Router**: プライベートネットワーク内の中継サーバー

### データフロー
1. **トンネル確立**: `conduit start` → Tunnel Process起動 → Process Registry登録 → Router接続
2. **データ転送**: 外部ユーザー → Tunnel Process(`--bind`) → Router(TLS) → Target Service(`--source`)
3. **制御コマンド**: `conduit list/kill/status` → Process Registry検索 → gRPC通信 → 結果表示

## 技術スタック

### コア技術
- **言語**: Rust (edition 2021, rust-version 1.70+)
- **非同期ランタイム**: Tokio
- **TLS実装**: rustls (TLS 1.3)
- **暗号化**: Ed25519 (32バイト鍵、128bit相当セキュリティ)
- **gRPC**: tonic, prost (プロトコルバッファ)
- **CLI**: clap v4
- **設定**: TOML (serde)
- **コンテナ**: Docker, Kubernetes

### 依存関係構成
```toml
[dependencies]
tokio = { version = "1.35", features = ["full"] }
rustls = { version = "0.21", features = ["dangerous_configuration"] }
ed25519-dalek = { version = "2.0", features = ["rand_core"] }
tonic = "0.10"
prost = "0.12"
prost-types = "0.12"
clap = { version = "4.4", features = ["derive", "color", "suggestions"] }
serde = { version = "1.0", features = ["derive"] }
dirs = "5.0"
# ... その他多数

[build-dependencies]
tonic-build = "0.10"
```

## コマンド仕様（シンプル化済み）

### 基本コマンド体系
```bash
conduit <SUBCOMMAND> [OPTIONS]
```

### 全コマンド一覧（10コマンド）
| コマンド | 説明 | 用途 |
|---------|------|------|
| `init` | 初期化・キーペア生成 | プロジェクトセットアップ |
| `start` | 単発トンネル開始 | Client経由でRouter側サービスへのアクセス経路を作成 |
| `up` | 設定ファイルから一括起動 | 複数のRouter側サービスへのアクセス経路を提供 |
| `down` | トンネル群停止 | `up`で起動したトンネルを一括停止 |
| `router` | ルーター起動 | Routerサーバーの運用 |
| `list` | トンネル・接続一覧表示 | アクティブなトンネルと接続状況の確認 |
| `kill` | トンネル・接続終了 | 特定のトンネルまたは接続の強制終了 |
| `status` | システム状況確認 | Router、Clientの稼働状況確認 |
| `config` | 設定管理 | 設定の表示・検証・生成 |
| `version` | バージョン情報表示 | Conduitのバージョンとビルド情報表示 |

### 使用例
```bash
# 初期化
conduit init

# 単発トンネル（デーモンレス）
conduit start --router 10.2.0.1:9999 \
              --source 10.2.0.2:8080 \
              --bind 0.0.0.0:80

# 設定ファイルから起動
conduit up -f conduit.toml

# ルーター起動
conduit router --bind 0.0.0.0:9999

# 制御コマンド（gRPC経由）
conduit list
conduit kill --tunnel web-server-access
conduit status

# サービスファイル生成
conduit start --service-file systemd --router 10.2.0.1:9999 --source 10.2.0.2:8080 --bind 0.0.0.0:80
```

## 設定システム

### 階層的設定管理
**優先順位**: CLI引数 > 環境変数 > 設定ファイル > デフォルト値

### 設定ファイル構造（正しい仕様）
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

### 環境変数
```bash
# ルーター設定
export CONDUIT_ROUTER_HOST="10.2.0.1"
export CONDUIT_ROUTER_PORT="9999"

# セキュリティ設定
export CONDUIT_SECURITY_PRIVATE_KEY_PATH="./keys/client.key"
```

## セキュリティ仕様

### 暗号化アーキテクチャ
- **プロトコル**: TLS 1.3
- **公開鍵暗号**: Ed25519（32バイト鍵、高性能）
- **認証**: Ed25519デジタル署名 + トークンベース認証
- **鍵ローテーション**: 中央集権型、グレースピリオド付き

### 認証フロー
1. ClientHello（クライアント公開鍵送信）
2. ServerHello（サーバー能力通知）
3. AuthRequest（トークン + Ed25519署名）
4. AuthResponse（セッションID + 権限）

## パフォーマンス目標

### 性能要件
- **同時接続数**: 10,000+
- **スループット**: 10Gbps+
- **レイテンシ**: <10ms オーバーヘッド
- **CPU使用率**: <50% (8コア)
- **メモリ使用量**: <1GB

### 最適化手法
- Zero-copy networking
- 接続プールリング
- 適応的バッファサイズ
- CPU親和性設定

## 監視・運用機能

### メトリクス（Prometheus形式）
- `conduit_connections_total` - 総接続数
- `conduit_active_connections` - アクティブ接続数
- `conduit_bytes_transferred_total` - 転送バイト数
- `conduit_request_duration_seconds` - リクエスト処理時間

### ヘルスチェック
- HTTP GET `/health` エンドポイント
- ターゲットサーバーへの定期的な接続確認
- 異常時のアラート送信（Webhook対応）

### API仕様（REST）
```
GET /api/v1/info          # サービス情報
GET /api/v1/health        # ヘルスチェック
GET /api/v1/tunnels       # トンネル一覧
POST /api/v1/tunnels      # トンネル作成
GET /api/v1/connections   # 接続一覧
```

## プロジェクト構造

### デーモンレス実装ディレクトリ構成
```
conduit/
├── Cargo.toml                     # プロジェクト設定
├── src/
│   ├── main.rs                    # エントリポイント
│   ├── grpc/                      # gRPC通信（CLI ↔ Tunnel Process）
│   │   ├── mod.rs
│   │   ├── server.rs              # gRPCサーバー実装
│   │   ├── client.rs              # gRPCクライアント実装
│   │   └── tunnel.proto           # プロトコル定義
│   ├── registry/                  # Process Registry
│   │   ├── mod.rs
│   │   ├── manager.rs             # レジストリ管理
│   │   └── process.rs             # プロセス情報
│   ├── tunnel/                    # Tunnel Process
│   │   ├── mod.rs
│   │   ├── process.rs             # トンネルプロセス
│   │   └── manager.rs             # トンネル管理
│   ├── service/                   # サービスファイル生成
│   │   ├── mod.rs
│   │   ├── template.rs            # サービスファイルテンプレート
│   │   └── generator.rs           # サービスファイル生成
│   ├── cli/                       # CLI関連
│   ├── client/                    # クライアント実装
│   ├── router/                    # ルーター実装
│   ├── security/                  # セキュリティ（TLS, Keys, Auth）
│   ├── config/                    # 設定管理
│   ├── protocol/                  # プロトコル実装
│   ├── monitoring/                # 監視・メトリクス
│   └── common/                    # 共通ユーティリティ
├── tests/                         # テスト
├── docs/                          # ドキュメント
└── .github/                       # CI/CD設定
```

## デーモンレス実装計画（2025-06-15決定）

### Phase 1: gRPC Infrastructure (P0) - 2週間
- Week 1: gRPCプロトコル定義とコード生成
- Week 2: Process Registry実装

### Phase 2: Command Implementation (P0) - 2週間
- Week 3: CLI Commands修正（バックグラウンドプロセス起動）
- Week 4: Service File Generation機能

### Phase 3: Integration & Testing (P0) - 1週間
- Week 5: 統合テスト・品質保証

### Phase 4: Advanced Features (P1) - 2週間
- Week 6-7: 監視・メトリクス、API実装

## テスト戦略

### テスト種別
- **単体テスト**: カバレッジ90%以上目標
- **統合テスト**: エンドツーエンドシナリオ
- **パフォーマンステスト**: 負荷・レイテンシ・スループット
- **セキュリティテスト**: 認証・暗号化・入力検証

### CI/CDパイプライン
- GitHub Actions使用
- マルチプラットフォームビルド（Linux/macOS/Windows）
- 自動リリース（バイナリ + Dockerイメージ）

## 開発ガイドライン

### コーディング規約
- Rust標準：snake_case関数、PascalCase構造体
- `cargo fmt`、`cargo clippy`必須
- 構造化ログ（tracing使用）
- エラーは全てResult型で処理

### 品質基準
- 全CI/CDチェック通過
- コードカバレッジ90%以上
- セキュリティ監査通過
- ドキュメント完備

## デプロイメント対応

### プラットフォーム
- **OS**: Linux（Ubuntu, CentOS, Alpine）、macOS、Windows
- **アーキテクチャ**: x86_64、ARM64（Apple Silicon、AWS Graviton）
- **コンテナ**: Docker、Kubernetes
- **サービス管理**: systemd

### パッケージング
- 単一バイナリリリース
- マルチプラットフォームDockerイメージ
- GitHub Releases配布

## 理解すべき重要概念

### 設計哲学
- **Docker/Docker Compose風のUX**: 単発実行と設定ファイルベース実行
- **階層的設定管理**: CLI > 環境変数 > 設定ファイル > デフォルト
- **ヘッドレス対応**: CI/CD、Docker、Kubernetesでの完全自動化

### ユースケース（正しい用途）
- **リモートサーバーへの安全なアクセス**: プライベートネットワーク内のDBやAPIサーバーにインターネット経由でアクセス
- **開発環境への外部アクセス**: 外部の共同作業者がClient経由でRouter側の開発サーバーにアクセス
- **プライベートクラウドリソースへの接続**: NAT越しでRouter側のクラウドサービスやマイクロサービスに接続
- **セキュアなリバースプロキシ**: Router側のサービス群への暗号化されたアクセス経路を提供

## 次のステップ

### 開発開始時の作業順序
1. Rustプロジェクトセットアップ（`cargo new conduit`）
2. 基本CLI構造実装（clap使用）
3. 設定システム実装（TOML + serde）
4. エラーハンドリング実装（thiserror使用）
5. TLS + Ed25519セキュリティ基盤実装

### 重要なファイル
- `docs/architecture.md`: 完全な技術仕様（約36,000文字、71%削減済み）
- `.clinerules`: プロジェクト固有の開発ルール
- `LICENSE`: Apache-2.0 + SUSHI-WARE デュアルライセンス

## 注意事項

### 開発時の重要ポイント
- アーキテクチャドキュメントが最優先仕様書
- すべての実装は`docs/architecture.md`に準拠すること
- **正しい仕様**: 外部ユーザー → Client(`--bind`) → Router(TLS) → Router側サービス(`--source`)
- セキュリティ要件（TLS 1.3 + Ed25519）は妥協不可
- パフォーマンス目標（10,000+接続、10Gbps+）を常に意識
## デーモンレスアーキテクチャ設計学習（2025-06-15）

### 重要な設計決定
- **アーキテクチャ方針**: Podmanライクなデーモンレス設計を採用
- **通信プロトコル**: CLI ↔ Tunnel Process間はgRPC（全プラットフォーム対応）
- **プロセス管理**: ファイルベースのProcess Registry（`~/.conduit/tunnels/`）
- **サービスファイル生成**: `--service-file`フラグで複数プラットフォーム対応

### デーモンレス設計の利点
- **シンプルな管理**: 中央管理デーモンが不要
- **独立性**: 各トンネルが独立したプロセスとして動作
- **スケーラビリティ**: プロセス単位での管理が可能
- **障害分離**: 一つのトンネル障害が他に影響しない

### gRPC API設計
```protobuf
service TunnelControl {
  rpc GetStatus(StatusRequest) returns (StatusResponse);
  rpc ListConnections(ListRequest) returns (ListResponse);
  rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
  rpc GetMetrics(MetricsRequest) returns (MetricsResponse);
}
```

### Process Registry形式
```json
{
  "tunnel_id": "web-server-access-1234",
  "name": "web-server-access",
  "pid": 12345,
  "grpc_port": 50001,
  "config": { "router": "10.2.0.1:9999", "source": "10.2.0.2:8080", "bind": "0.0.0.0:80" },
  "status": "running",
  "created_at": "2025-06-15T03:35:00Z"
}
```

### サービスファイル生成対応
- **systemd** (Linux)
- **openrc** (Alpine Linux) 
- **launchd** (macOS)
- **rc.d** (FreeBSD)

### 実装上の技術的課題
- gRPCプロトコルバッファの適切な設計
- プロセス間通信のエラーハンドリング
- ファイルベースレジストリの排他制御
- マルチプラットフォーム対応のサービスファイル生成

### 開発チームへの引き継ぎ事項
1. 既存のCLIコマンド実装を修正してgRPC通信に対応
2. Process Registry機能の新規実装
3. Tunnel Processのバックグラウンド起動機能
4. サービスファイル生成機能の実装
5. 統合テストによる動作確認
- Docker/Kubernetes環境での動作を前提とした設計

### 重要な仕様変更（2025-06-14更新）
- **`--target` → `--source`**: 正しいオプション名に修正済み
- **Daemon機能削除**: 複雑性排除によりシンプルな10コマンド体系に変更
- **設定ファイル環境管理**: profile機能廃止、ファイル分離方式に簡素化

### 技術的な課題
- 非同期処理の複雑性（Tokio習熟必要）
- TLS実装の正確性（セキュリティクリティカル）
- 高性能要件（Zero-copy、最適化必要）
- クロスプラットフォーム対応（ビルド・テスト複雑）

### 開発ガイドライン（重要）
**コーディングスタイル規約**:
- **コメント**: 日本語で記述する
- **出力メッセージ**: format!()、println!()、エラーメッセージなどは英語で記述する
- **一貫性**: この規約をプロジェクト全体で厳密に遵守する

### セキュリティ基盤実装状況（2025-06-15実装完了）

**実装済みモジュール**:
- `src/security/mod.rs` - セキュリティモジュール統合
- `src/security/crypto.rs` - Ed25519暗号化・署名実装（32バイトキー）
- `src/security/tls.rs` - TLS 1.3設定（rustls使用）
- `src/security/keys.rs` - 鍵管理・ローテーション（30日間隔）
- `src/security/auth.rs` - 認証・認可システム

**主要機能**:
- Ed25519キーペア生成・管理
- TLS 1.3クライアント・サーバー設定
- 30日間隔の自動鍵ローテーション
- トークンベース認証
- セッション管理

**技術仕様**:
- Ed25519: 32バイトキー、128bit相当セキュリティ
- TLS 1.3: rustls使用、相互認証対応
- 鍵ローテーション: 30日間隔、24時間グレースピリオド
- Base64エンコーディング: 鍵・署名の保存形式

このプロジェクトは企業級の高性能ネットワークトンネリングソフトウェアとして、セキュリティ、パフォーマンス、運用性のすべてにおいて高い水準を目指しています。