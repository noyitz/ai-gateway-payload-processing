/*
Copyright 2026 The opendatahub.io Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

package auth

import (
	"context"
	"fmt"
	"sync"
	"time"

	"golang.org/x/oauth2/google"
)

const (
	serviceAccountKeyField = "service-account-key"
	tokenRefreshBuffer     = 5 * time.Minute
)

var _ AuthHeadersGenerator = &OAuthAuthGenerator{}

type cachedToken struct {
	accessToken string
	expiry      time.Time
}

// OAuthAuthGenerator exchanges a GCP Service Account JSON key for an OAuth
// access token and returns it as a Bearer header. Tokens are cached in memory
// and refreshed automatically when near expiry.
type OAuthAuthGenerator struct {
	HeaderName string
	Scope      string

	mu    sync.RWMutex
	cache map[string]*cachedToken
}

// GenerateAuthHeaders reads the "service-account-key" field from credentials
// (a GCP Service Account JSON key), exchanges it for an OAuth token, and
// returns {"Authorization": "Bearer <token>"}.
func (g *OAuthAuthGenerator) GenerateAuthHeaders(credentials map[string]string) (map[string]string, error) {
	saKey, ok := credentials[serviceAccountKeyField]
	if !ok {
		return nil, fmt.Errorf("credentials missing required field %s", serviceAccountKeyField)
	}

	token, err := g.getOrRefreshToken(saKey)
	if err != nil {
		return nil, fmt.Errorf("failed to get OAuth token: %w", err)
	}

	return map[string]string{
		g.HeaderName: fmt.Sprintf("Bearer %s", token),
	}, nil
}

func (g *OAuthAuthGenerator) getOrRefreshToken(saKey string) (string, error) {
	g.mu.RLock()
	if g.cache != nil {
		if cached, ok := g.cache[saKey]; ok && time.Now().Before(cached.expiry.Add(-tokenRefreshBuffer)) {
			g.mu.RUnlock()
			return cached.accessToken, nil
		}
	}
	g.mu.RUnlock()

	g.mu.Lock()
	defer g.mu.Unlock()

	// Double-check after acquiring write lock
	if g.cache != nil {
		if cached, ok := g.cache[saKey]; ok && time.Now().Before(cached.expiry.Add(-tokenRefreshBuffer)) {
			return cached.accessToken, nil
		}
	}

	creds, err := google.CredentialsFromJSON(context.Background(), []byte(saKey), g.Scope)
	if err != nil {
		return "", fmt.Errorf("failed to parse service account key: %w", err)
	}

	tok, err := creds.TokenSource.Token()
	if err != nil {
		return "", fmt.Errorf("failed to exchange service account key for token: %w", err)
	}

	if g.cache == nil {
		g.cache = make(map[string]*cachedToken)
	}
	g.cache[saKey] = &cachedToken{
		accessToken: tok.AccessToken,
		expiry:      tok.Expiry,
	}

	return tok.AccessToken, nil
}
