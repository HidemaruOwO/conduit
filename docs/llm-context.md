# Conduit プロジェクト - LLM コンテキスト

最終更新: 2025-06-12 08:22 JST  
更新者: LLM アシスタント

## プロジェクト概要

### 現在の状況
- **ステータス**: 開発未着手（設計フェーズ完了）
- **プロジェクトタイプ**: Rustで開発される企業級ネットワークトンネリングソフトウェア
- **リポジトリ構造**: MicroRepositoryテンプレートベース
- **主要ドキュメント**: `docs/architecture.md`に4682行の完全な技術仕様が記載
- **ライセンス**: Apache 2.0 と SUSHI-WARE のデュアルライセンス

## プロジェクトの理解

### Conduitとは
Conduitは「導管」を意味し、異なるネットワークセグメント間で安全で高性能なTCP/UDPポートフォワーディングを提供するRust製ソフトウェアです。

### 主要な特徴
- **高性能**: Rust + Tokio による非同期処理で数万の同時接続をサポート
- **セキュア**: TLS 1.3 + Ed25519 による暗号化
- **運用性**: Docker/Kubernetes ネイティブ対応
- **可観測性**: 包括的な監視・ログ機能
- **単一バイナリ**: クライアント・ルーター統合コマンド

## アーキテクチャ理解

### システム構成
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Target Zone   │    │  Router Zone    │    │   Source Zone   │
│  10.1.0.0/24   │    │  10.2.0.0/24   │    │  10.2.0.0/24   │
│                 │    │                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │   Target    │◄├────┤ │   Router    │◄├────┤ │   Client    │ │
│ │ 10.1.0.1:80 │ │    │ │10.2.0.1:9999│ │    │ │10.2.0.2:8080│ │
│ └─────────────┘ │    │ └─────────────┘ │    │ └─────────────┘ │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### 主要コンポーネント
1. **Conduit Client**: ローカルポートにバインドし、トラフィックをルーター経由でターゲットに転送
2. **Conduit Router**: 中継サーバー、複数クライアントからの接続を受け付けてターゲットへプロキシ
3. **Target**: 転送先の実際のサービスサーバー

### データフロー
1. Client → Router: TLS Handshake + Ed25519認証
2. Client → Router: Tunnel Request
3. Router → Target: TCP Connection
4. Data Relay: Client ↔ Router ↔ Target

## 技術スタック

### コア技術
- **言語**: Rust (edition 2021, rust-version 1.70+)
- **非同期ランタイム**: Tokio
- **TLS実装**: rustls (TLS 1.3)
- **暗号化**: Ed25519 (32バイト鍵、128bit相当セキュリティ)
- **CLI**: clap v4
- **設定**: TOML (serde)
- **コンテナ**: Docker, Kubernetes

### 依存関係構成
```toml
[dependencies]
tokio = { version = "1.35", features = ["full"] }
rustls = { version = "0.21", features = ["dangerous_configuration"] }
ed25519-dalek = { version = "2.0", features = ["rand_core"] }
clap = { version = "4.4", features = ["derive", "color", "suggestions"] }
serde = { version = "1.0", features = ["derive"] }
# ... その他多数
```

## コマンド仕様

### 基本コマンド体系
```bash
conduit <SUBCOMMAND> [OPTIONS]
```

### 主要コマンド（実装優先度P0）
| コマンド | 説明 | 用途 |
|---------|------|------|
| `init` | 初期化・キーペア生成 | プロジェクトセットアップ |
| `start` | 単発トンネル開始 | Docker風の単発実行 |
| `up` | 設定ファイルから一括起動 | Docker Compose風の一括実行 |
| `down` | トンネル群停止 | サービス停止 |
| `router` | ルーター起動 | 中継サーバー起動 |
| `list` | アクティブ接続一覧 | 状態確認 |
| `kill` | 接続終了 | 個別接続管理 |
| `config` | 設定管理 | 設定の検証・編集 |

### 使用例
```bash
# 初期化
conduit init

# 単発トンネル（Docker風）
conduit start --router router.example.com:9999 \
              --target 10.1.0.1:80 \
              --bind 0.0.0.0:8080

# 設定ファイルから起動（Docker Compose風）
conduit up --file conduit.toml

# ルーター起動
conduit router --bind 0.0.0.0:9999 \
               --cert /certs/server.crt \
               --key /certs/server.key
```

## 設定システム

### 階層的設定管理
**優先順位**: CLI引数 > 環境変数 > 設定ファイル > デフォルト値

### 設定ファイル構造
```toml
# conduit.toml
version = "2.0"

[router]
host = "router.example.com"
port = 9999

[security]
private_key_path = "./keys/client.key"
router_token_path = "./tokens/router.token"
tls_version = "1.3"

[logging]
level = "info"
format = "json"

[[tunnels]]
name = "web-server"
target = "10.1.0.1:80"
bind = "0.0.0.0:8080"
protocol = "tcp"
auto_start = true
```

### 環境変数
```bash
# ルーター設定
export CONDUIT_ROUTER_HOST="router.example.com"
export CONDUIT_ROUTER_PORT="9999"

# セキュリティ設定
export CONDUIT_SECURITY_PRIVATE_KEY_PATH="./keys/client.key"
export CONDUIT_SECURITY_ROUTER_TOKEN_PATH="./tokens/router.token"
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

### ディレクトリ構成（予定）
```
conduit/
├── Cargo.toml                     # プロジェクト設定
├── src/
│   ├── main.rs                    # エントリポイント
│   ├── cli/                       # CLI関連
│   ├── client/                    # クライアント実装
│   ├── router/                    # ルーター実装
│   ├── security/                  # セキュリティ（TLS, Keys, Auth）
│   ├── config/                    # 設定管理
│   ├── protocol/                  # プロトコル実装
│   ├── monitoring/                # 監視・メトリクス
│   ├── api/                       # REST API
│   └── utils/                     # ユーティリティ
├── tests/                         # テスト
├── docs/                          # ドキュメント
├── docker/                        # Docker関連
└── .github/                       # CI/CD設定
```

## 実装計画

### Phase 1: Core Implementation (P0) - 4週間
- Week 1: 基盤実装（CLI、設定、エラーハンドリング）
- Week 2: セキュリティ実装（TLS、Ed25519、認証）
- Week 3: コア通信実装（プロトコル、基本トンネル機能）
- Week 4: ルーター実装

### Phase 2: Essential Features (P0) - 3週間
- Week 5-6: コマンド実装（init, start, up, router等）
- Week 7: 監視・メトリクス基盤

### Phase 3: Advanced Features (P1) - 4週間
- Week 8-9: ヘルスチェック・アラート
- Week 10: API実装
- Week 11: プロファイル管理

### Phase 4: Production Features (P2) - 3週間
- Week 12-13: 高度な監視機能
- Week 14: 運用機能

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

### ユースケース
- マイクロサービス間通信
- ハイブリッドクラウド接続
- 開発環境（ローカル開発環境とクラウドリソースの接続）
- レガシーシステム統合

## 次のステップ

### 開発開始時の作業順序
1. Rustプロジェクトセットアップ（`cargo new conduit`）
2. 基本CLI構造実装（clap使用）
3. 設定システム実装（TOML + serde）
4. エラーハンドリング実装（thiserror使用）
5. TLS + Ed25519セキュリティ基盤実装

### 重要なファイル
- `docs/architecture.md`: 完全な技術仕様（4682行）
- `.clinerules`: プロジェクト固有の開発ルール
- `LICENSE`: Apache-2.0 + SUSHI-WARE デュアルライセンス

## 注意事項

### 開発時の重要ポイント
- アーキテクチャドキュメントが最優先仕様書
- すべての実装は`docs/architecture.md`に準拠すること
- セキュリティ要件（TLS 1.3 + Ed25519）は妥協不可
- パフォーマンス目標（10,000+接続、10Gbps+）を常に意識
- Docker/Kubernetes環境での動作を前提とした設計

### 技術的な課題
- 非同期処理の複雑性（Tokio習熟必要）
- TLS実装の正確性（セキュリティクリティカル）
- 高性能要件（Zero-copy、最適化必要）
- クロスプラットフォーム対応（ビルド・テスト複雑）

このプロジェクトは企業級の高性能ネットワークトンネリングソフトウェアとして、セキュリティ、パフォーマンス、運用性のすべてにおいて高い水準を目指しています。