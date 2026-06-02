#!/bin/bash
# Swap ext-claude-sonnet back to Anthropic API
oc patch externalmodel ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"endpoint":"api.anthropic.com","credentialRef":{"name":"anthropic-api-key"},"targetModel":"claude-opus-4-6"}}'
oc patch service ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"externalName":"api.anthropic.com"}}'
oc patch httproute ext-claude-sonnet -n llm --type=json -p '[
  {"op":"replace","path":"/spec/rules/0/filters/0/requestHeaderModifier/set/0/value","value":"api.anthropic.com"},
  {"op":"replace","path":"/spec/rules/1/filters/0/requestHeaderModifier/set/0/value","value":"api.anthropic.com"}
]'
echo "Swapped to ANTHROPIC (claude-opus-4-6)"
