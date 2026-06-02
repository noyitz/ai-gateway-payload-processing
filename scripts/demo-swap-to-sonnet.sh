#!/bin/bash
# Swap ext-claude-sonnet to Sonnet (cheaper model demo)
# Same endpoint and credentials, just different targetModel
oc patch externalmodel ext-claude-sonnet -n llm --type=merge \
  -p '{"spec":{"targetModel":"claude-sonnet-4-20250514"}}'
echo "Swapped to SONNET (claude-sonnet-4-20250514)"
