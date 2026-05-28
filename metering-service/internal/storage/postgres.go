package storage

import (
	"context"
	"database/sql"
	"fmt"
	"log/slog"
	"time"

	_ "github.com/lib/pq"
)

type UsageEvent struct {
	EventID          string
	Timestamp        time.Time
	Username         string
	GroupName        string
	Subscription     string
	Provider         string
	Model            string
	PromptTokens     int
	CompletionTokens int
	TotalTokens      int
	Source           string
}

type UsageStats struct {
	HasAccess bool    `json:"hasAccess"`
	Balance   float64 `json:"balance"`
	Usage     float64 `json:"usage"`
	Overage   float64 `json:"overage"`
}

type Store struct {
	db *sql.DB
}

func New(databaseURL string) (*Store, error) {
	db, err := sql.Open("postgres", databaseURL)
	if err != nil {
		return nil, fmt.Errorf("open database: %w", err)
	}

	db.SetMaxOpenConns(10)
	db.SetMaxIdleConns(5)
	db.SetConnMaxLifetime(14400 * time.Second)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := db.PingContext(ctx); err != nil {
		return nil, fmt.Errorf("ping database: %w", err)
	}

	s := &Store{db: db}
	if err := s.migrate(ctx); err != nil {
		return nil, fmt.Errorf("migrate: %w", err)
	}

	return s, nil
}

func (s *Store) Close() error {
	return s.db.Close()
}

func (s *Store) InsertEvent(ctx context.Context, e UsageEvent) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO usage_events (event_id, timestamp, username, group_name, subscription, provider, model, prompt_tokens, completion_tokens, total_tokens, source)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`,
		e.EventID, e.Timestamp, e.Username, e.GroupName, e.Subscription, e.Provider, e.Model,
		e.PromptTokens, e.CompletionTokens, e.TotalTokens, e.Source,
	)
	return err
}

func (s *Store) GetMonthlyUsage(ctx context.Context, username, model string) (UsageStats, error) {
	var used int64
	row := s.db.QueryRowContext(ctx,
		`SELECT COALESCE(SUM(total_tokens), 0) FROM usage_events
		 WHERE username = $1 AND timestamp >= date_trunc('month', NOW())`,
		username,
	)
	if err := row.Scan(&used); err != nil {
		return UsageStats{}, err
	}

	quota := float64(1000000)
	usage := float64(used)
	balance := quota - usage
	overage := float64(0)
	if balance < 0 {
		overage = -balance
		balance = 0
	}

	return UsageStats{
		HasAccess: usage < quota,
		Balance:   balance,
		Usage:     usage,
		Overage:   overage,
	}, nil
}

func (s *Store) migrate(ctx context.Context) error {
	slog.Info("running database migrations")
	for _, stmt := range migrations {
		if _, err := s.db.ExecContext(ctx, stmt); err != nil {
			return fmt.Errorf("migration failed: %w", err)
		}
	}
	slog.Info("database migrations complete")
	return nil
}

var migrations = []string{
	`CREATE TABLE IF NOT EXISTS usage_events (
		id BIGSERIAL PRIMARY KEY,
		event_id TEXT NOT NULL,
		timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
		username TEXT NOT NULL,
		group_name TEXT,
		subscription TEXT,
		provider TEXT NOT NULL DEFAULT '',
		model TEXT NOT NULL,
		prompt_tokens INTEGER NOT NULL DEFAULT 0,
		completion_tokens INTEGER NOT NULL DEFAULT 0,
		total_tokens INTEGER NOT NULL DEFAULT 0,
		source TEXT DEFAULT 'maas-gateway'
	)`,
	`CREATE INDEX IF NOT EXISTS idx_usage_events_timestamp ON usage_events (timestamp)`,
	`CREATE INDEX IF NOT EXISTS idx_usage_events_username ON usage_events (username)`,
	`CREATE INDEX IF NOT EXISTS idx_usage_events_group ON usage_events (group_name)`,
	`CREATE INDEX IF NOT EXISTS idx_usage_events_model ON usage_events (model)`,
	`CREATE INDEX IF NOT EXISTS idx_usage_events_ts_user_model ON usage_events (timestamp, username, model)`,
	`CREATE TABLE IF NOT EXISTS model_pricing (
		model TEXT PRIMARY KEY,
		provider TEXT NOT NULL,
		prompt_cost_per_1k NUMERIC(10,6) NOT NULL DEFAULT 0,
		completion_cost_per_1k NUMERIC(10,6) NOT NULL DEFAULT 0
	)`,
	`INSERT INTO model_pricing (model, provider, prompt_cost_per_1k, completion_cost_per_1k) VALUES
		('gpt-4o', 'openai', 0.0025, 0.01),
		('gpt-4o-mini', 'openai', 0.00015, 0.0006),
		('gpt-4.1', 'openai', 0.002, 0.008),
		('gpt-4.1-mini', 'openai', 0.0004, 0.0016),
		('claude-opus-4-6', 'anthropic', 0.015, 0.075),
		('claude-sonnet-4-20250514', 'anthropic', 0.003, 0.015),
		('claude-haiku-4-5-20251001', 'anthropic', 0.0008, 0.004),
		('gemini-2.0-flash', 'vertex', 0.0001, 0.0004),
		('gemini-2.5-pro', 'vertex', 0.00125, 0.01)
	ON CONFLICT (model) DO NOTHING`,
}
