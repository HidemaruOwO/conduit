-- Conduit Process Registry Database Schema

-- トンネルプロセス管理テーブル
CREATE TABLE IF NOT EXISTS tunnels (
    id TEXT PRIMARY KEY,                    -- トンネルID (UUID)
    name TEXT NOT NULL,                     -- トンネル名（ユーザー指定）
    pid INTEGER DEFAULT NULL,               -- プロセスID（終了時はNULL）
    socket_path_hash TEXT NOT NULL,        -- Unix Domain Socket パス（ハッシュ化）
    status INTEGER NOT NULL,                -- Podmanライクな数値状態
    config_encrypted BLOB,                  -- 設定情報（暗号化済み）
    config_checksum TEXT NOT NULL,         -- 設定の完全性チェック用ハッシュ
    created_at INTEGER NOT NULL,            -- 作成日時（Unix timestamp）
    updated_at INTEGER NOT NULL,            -- 更新日時（Unix timestamp）
    last_activity INTEGER NOT NULL,        -- 最終アクティビティ時刻
    exit_code INTEGER DEFAULT NULL,        -- 終了コード（終了時のみ）
    
    -- 整合性制約: ステータスと終了コードの関係
    CHECK (
        (status IN (1, 3, 4) AND exit_code IS NULL) OR 
        (status IN (5, 6) AND exit_code IS NOT NULL)
    ),
    
    -- PID制約: 実行中状態の場合PIDは必須
    CHECK (
        (status = 3 AND pid IS NOT NULL) OR 
        (status != 3)
    )
);

-- クライアント接続管理テーブル（監査強化）
CREATE TABLE IF NOT EXISTS clients (
    id TEXT PRIMARY KEY,                    -- 接続ID (UUID)
    tunnel_id TEXT NOT NULL,               -- 所属トンネルID
    client_addr_hash TEXT NOT NULL,        -- クライアントアドレス（ハッシュ化）
    target_addr_hash TEXT NOT NULL,        -- ターゲットアドレス（ハッシュ化）
    connected_at INTEGER NOT NULL,         -- 接続開始時刻
    disconnected_at INTEGER DEFAULT NULL,  -- 切断時刻
    last_activity INTEGER NOT NULL,        -- 最終アクティビティ時刻
    bytes_sent INTEGER DEFAULT 0,          -- 送信バイト数
    bytes_received INTEGER DEFAULT 0,      -- 受信バイト数
    status TEXT NOT NULL DEFAULT 'active', -- 接続状態
    session_timeout INTEGER NOT NULL DEFAULT 3600, -- セッションタイムアウト（秒）
    
    FOREIGN KEY (tunnel_id) REFERENCES tunnels (id) ON DELETE CASCADE,
    
    -- セッションタイムアウト制約
    CHECK (session_timeout > 0 AND session_timeout <= 86400)
);

-- セッション統計テーブル（集計データ）
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,                    -- セッションID
    tunnel_id TEXT NOT NULL,               -- 所属トンネルID
    started_at INTEGER NOT NULL,           -- セッション開始時刻
    ended_at INTEGER DEFAULT NULL,         -- セッション終了時刻
    total_connections INTEGER DEFAULT 0,   -- 総接続数
    total_bytes_sent INTEGER DEFAULT 0,    -- 総送信バイト数
    total_bytes_received INTEGER DEFAULT 0, -- 総受信バイト数
    avg_latency_ms REAL DEFAULT 0.0,      -- 平均レイテンシ
    error_count INTEGER DEFAULT 0,         -- エラー回数
    
    FOREIGN KEY (tunnel_id) REFERENCES tunnels (id) ON DELETE CASCADE,
    
    -- データ完全性制約
    CHECK (total_connections >= 0),
    CHECK (total_bytes_sent >= 0),
    CHECK (total_bytes_received >= 0),
    CHECK (avg_latency_ms >= 0.0),
    CHECK (error_count >= 0)
);

-- 監査ログテーブル（セキュリティ監査）
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL,                   -- 操作種別
    target_table TEXT NOT NULL,            -- 対象テーブル
    target_id TEXT,                        -- 対象レコードID
    user_context TEXT,                     -- ユーザーコンテキスト（ハッシュ化）
    timestamp INTEGER NOT NULL,            -- 操作時刻
    success BOOLEAN NOT NULL DEFAULT TRUE, -- 操作成功/失敗
    error_message TEXT,                    -- エラーメッセージ（存在する場合）
    
    -- 監査データは削除不可
    CHECK (action IN ('CREATE', 'UPDATE', 'DELETE', 'SELECT_SENSITIVE'))
);

-- 設定メタデータテーブル（暗号化キー管理）
CREATE TABLE IF NOT EXISTS config_metadata (
    key_id TEXT PRIMARY KEY,               -- キーID
    algorithm TEXT NOT NULL,               -- 暗号化アルゴリズム
    key_rotation_at INTEGER NOT NULL,      -- キーローテーション予定日時
    created_at INTEGER NOT NULL,           -- キー作成日時
    is_active BOOLEAN NOT NULL DEFAULT TRUE, -- アクティブフラグ
    
    CHECK (algorithm IN ('AES-256-GCM', 'ChaCha20-Poly1305'))
);

-- 高性能・セキュアインデックス設計
-- 状態別の部分インデックス（パフォーマンス向上）
CREATE INDEX IF NOT EXISTS idx_tunnels_running ON tunnels(id, updated_at) WHERE status = 3;
CREATE INDEX IF NOT EXISTS idx_tunnels_stopped ON tunnels(id, exit_code) WHERE status IN (5, 6);
CREATE INDEX IF NOT EXISTS idx_tunnels_created_at ON tunnels(created_at);

-- アクティブ接続の高速検索
CREATE INDEX IF NOT EXISTS idx_clients_active ON clients(tunnel_id, last_activity) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_clients_tunnel_id ON clients(tunnel_id);

-- セッション統計の効率的検索
CREATE INDEX IF NOT EXISTS idx_sessions_tunnel_active ON sessions(tunnel_id, started_at) WHERE ended_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_sessions_ended ON sessions(ended_at) WHERE ended_at IS NOT NULL;

-- 監査ログの時系列検索
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_action_target ON audit_log(action, target_table);

-- 自動クリーンアップトリガー（古いデータの削除）
CREATE TRIGGER IF NOT EXISTS auto_cleanup_sessions 
AFTER INSERT ON sessions
BEGIN
    -- 90日以上前の終了セッションデータを削除
    DELETE FROM sessions 
    WHERE ended_at IS NOT NULL 
    AND ended_at < unixepoch('now', '-90 days');
END;

CREATE TRIGGER IF NOT EXISTS auto_cleanup_audit_log 
AFTER INSERT ON audit_log
BEGIN
    -- 1年以上前の監査ログを削除（法的要件に応じて調整）
    DELETE FROM audit_log 
    WHERE timestamp < unixepoch('now', '-365 days');
END;

-- セッションタイムアウト検知トリガー
CREATE TRIGGER IF NOT EXISTS session_timeout_check 
AFTER UPDATE OF last_activity ON clients
FOR EACH ROW
WHEN NEW.status = 'active' 
AND NEW.last_activity < unixepoch('now', '-' || NEW.session_timeout || ' seconds')
BEGIN
    -- タイムアウト時に自動切断
    UPDATE clients 
    SET status = 'timeout', disconnected_at = unixepoch('now')
    WHERE id = NEW.id;
    
    -- 監査ログ記録
    INSERT INTO audit_log (action, target_table, target_id, timestamp, user_context)
    VALUES ('UPDATE', 'clients', NEW.id, unixepoch('now'), 'SYSTEM_TIMEOUT');
END;

-- WALモード有効化（セキュア設定）
PRAGMA journal_mode=WAL;
PRAGMA synchronous=FULL;                -- クラッシュ安全性最大化
PRAGMA cache_size=5000;                 -- メモリ使用量制限
PRAGMA temp_store=memory;
PRAGMA mmap_size=134217728;             -- 128MB mmap（制限値）
PRAGMA auto_vacuum=FULL;                -- フラグメンテーション対策
PRAGMA secure_delete=ON;                -- 削除データのゼロ書き込み
PRAGMA foreign_keys=ON;                 -- 外部キー制約有効化

-- 初期設定メタデータ
INSERT OR IGNORE INTO config_metadata (key_id, algorithm, key_rotation_at, created_at)
VALUES ('default', 'AES-256-GCM', unixepoch('now', '+30 days'), unixepoch('now'));