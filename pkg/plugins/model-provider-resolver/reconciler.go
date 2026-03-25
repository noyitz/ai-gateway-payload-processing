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

package model_provider_resolver

import (
	"context"
	"fmt"

	"k8s.io/apimachinery/pkg/api/errors"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	ctrl "sigs.k8s.io/controller-runtime"
	"sigs.k8s.io/controller-runtime/pkg/client"
	"sigs.k8s.io/controller-runtime/pkg/log"
)

// externalModelReconciler watches ExternalModel CRDs (via unstructured client)
// and updates the model store with provider and credential information.
type externalModelReconciler struct {
	client.Reader
	store *modelInfoStore
}

// Reconcile handles create/update/delete events for ExternalModel resources.
// The ExternalModel CR name is used as the model key in the store, matching
// the model name in inference request bodies.
func (r *externalModelReconciler) Reconcile(ctx context.Context, req ctrl.Request) (ctrl.Result, error) {
	logger := log.FromContext(ctx)
	logger.Info("Reconciling ExternalModel", "name", req.Name, "namespace", req.Namespace)

	obj := &unstructured.Unstructured{}
	obj.SetGroupVersionKind(externalModelGVK)

	err := r.Get(ctx, req.NamespacedName, obj)
	if errors.IsNotFound(err) {
		r.store.deleteByResource(req.NamespacedName)
		logger.Info("ExternalModel deleted, cleaned store", "name", req.Name)
		return ctrl.Result{}, nil
	}
	if err != nil {
		return ctrl.Result{}, fmt.Errorf("unable to get ExternalModel: %w", err)
	}

	if !obj.GetDeletionTimestamp().IsZero() {
		r.store.deleteByResource(req.NamespacedName)
		return ctrl.Result{}, nil
	}

	provider, _, _ := unstructured.NestedString(obj.Object, "spec", "provider")
	if provider == "" {
		logger.Info("ExternalModel missing provider, skipping", "name", req.Name)
		r.store.deleteByResource(req.NamespacedName)
		return ctrl.Result{}, nil
	}

	info := ModelInfo{
		provider: provider,
	}

	// Extract credentialRef
	credName, _, _ := unstructured.NestedString(obj.Object, "spec", "credentialRef", "name")
	credNS, _, _ := unstructured.NestedString(obj.Object, "spec", "credentialRef", "namespace")
	if credName != "" {
		info.credentialRefName = credName
		info.credentialRefNamespace = credNS
		if info.credentialRefNamespace == "" {
			info.credentialRefNamespace = obj.GetNamespace()
		}
	}

	// ExternalModel name = model key (matches the model name in request body)
	r.store.setModelInfo(req.Name, info, req.NamespacedName)
	logger.Info("Updated model store", "model", req.Name, "provider", provider)

	return ctrl.Result{}, nil
}
