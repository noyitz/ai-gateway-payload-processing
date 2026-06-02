#!/bin/bash
# Swap ext-claude-sonnet to the simulator (on-prem model demo)
oc patch externalmodel ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"endpoint":"3-147-232-199.sslip.io","credentialRef":{"name":"simulator-api-key"},"targetModel":"internal-on-prem"}}'
oc patch service ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"externalName":"3-147-232-199.sslip.io"}}'
oc patch httproute ext-claude-sonnet -n llm --type=json -p '[
  {"op":"replace","path":"/spec/rules/0/filters/0/requestHeaderModifier/set/0/value","value":"3-147-232-199.sslip.io"},
  {"op":"replace","path":"/spec/rules/1/filters/0/requestHeaderModifier/set/0/value","value":"3-147-232-199.sslip.io"}
]'
echo "Swapped to SIMULATOR (on-prem)"
