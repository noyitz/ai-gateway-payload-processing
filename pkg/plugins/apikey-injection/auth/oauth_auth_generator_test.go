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
	"strings"
	"testing"
)

func TestOAuthAuthGenerator(t *testing.T) {
	tests := []struct {
		name        string
		credentials map[string]string
		wantErrMsg  string
	}{
		{
			name:        "missing service-account-key field",
			credentials: map[string]string{"api-key": "some-key"},
			wantErrMsg:  "credentials missing required field service-account-key",
		},
		{
			name:        "empty credentials",
			credentials: map[string]string{},
			wantErrMsg:  "credentials missing required field service-account-key",
		},
		{
			name:        "invalid JSON in service-account-key",
			credentials: map[string]string{"service-account-key": "not-valid-json"},
			wantErrMsg:  "failed to parse service account key",
		},
		{
			name: "valid JSON but not a service account key",
			credentials: map[string]string{
				"service-account-key": `{"type":"wrong_type","project_id":"test"}`,
			},
			wantErrMsg: "failed to parse service account key",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			generator := &OAuthAuthGenerator{
				HeaderName: "Authorization",
				Scope:      "https://www.googleapis.com/auth/cloud-platform",
			}

			headers, err := generator.GenerateAuthHeaders(test.credentials)
			if err == nil {
				t.Fatalf("expected error containing %q but got nil (headers=%v)", test.wantErrMsg, headers)
			}
			if !strings.Contains(err.Error(), test.wantErrMsg) {
				t.Errorf("error %q does not contain %q", err.Error(), test.wantErrMsg)
			}
		})
	}
}
