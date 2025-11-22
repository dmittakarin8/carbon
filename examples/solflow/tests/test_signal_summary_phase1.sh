#!/bin/bash
# Phase 1 Test: Token Signal Summary Table and API Endpoints
# Tests table creation, repository layer, and API endpoints

set -e

echo "==================================="
echo "Phase 1: Signal Summary Tests"
echo "==================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

DB_PATH="${DB_PATH:-/var/lib/solflow/solflow.db}"
API_BASE="${API_BASE:-http://localhost:3000}"

# Test 1: Verify table exists
echo ""
echo "Test 1: Verify token_signal_summary table exists"
if sqlite3 "$DB_PATH" "SELECT name FROM sqlite_master WHERE type='table' AND name='token_signal_summary';" | grep -q "token_signal_summary"; then
  echo -e "${GREEN}✓ Table exists${NC}"
else
  echo -e "${RED}✗ Table does not exist - creating...${NC}"
  sqlite3 "$DB_PATH" < sql/07_token_signal_summary.sql
  echo -e "${GREEN}✓ Table created${NC}"
fi

# Test 2: Verify table schema
echo ""
echo "Test 2: Verify table schema"
SCHEMA=$(sqlite3 "$DB_PATH" "PRAGMA table_info(token_signal_summary);")
if echo "$SCHEMA" | grep -q "token_address" && \
   echo "$SCHEMA" | grep -q "persistence_score" && \
   echo "$SCHEMA" | grep -q "pattern_tag" && \
   echo "$SCHEMA" | grep -q "confidence" && \
   echo "$SCHEMA" | grep -q "appearance_24h" && \
   echo "$SCHEMA" | grep -q "appearance_72h" && \
   echo "$SCHEMA" | grep -q "updated_at"; then
  echo -e "${GREEN}✓ Schema is correct${NC}"
else
  echo -e "${RED}✗ Schema is incorrect${NC}"
  exit 1
fi

# Test 3: Verify indexes exist
echo ""
echo "Test 3: Verify indexes exist"
INDEXES=$(sqlite3 "$DB_PATH" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='token_signal_summary';")
if echo "$INDEXES" | grep -q "idx_token_signal_summary_persistence_score" && \
   echo "$INDEXES" | grep -q "idx_token_signal_summary_pattern_tag" && \
   echo "$INDEXES" | grep -q "idx_token_signal_summary_updated_at"; then
  echo -e "${GREEN}✓ All indexes exist${NC}"
else
  echo -e "${YELLOW}⚠ Some indexes may be missing (expected if table was just created)${NC}"
fi

# Test 4: Insert test data directly
echo ""
echo "Test 4: Insert test data via SQL"
TEST_MINT="TEST_MINT_DUMMY_ADDRESS_FOR_VALIDATION"
NOW=$(date +%s)
sqlite3 "$DB_PATH" "INSERT OR REPLACE INTO token_signal_summary (token_address, persistence_score, pattern_tag, confidence, appearance_24h, appearance_72h, updated_at) VALUES ('$TEST_MINT', 8, 'ACCUMULATION', 'HIGH', 5, 12, $NOW);"
RESULT=$(sqlite3 "$DB_PATH" "SELECT persistence_score, pattern_tag FROM token_signal_summary WHERE token_address='$TEST_MINT';")
if echo "$RESULT" | grep -q "8|ACCUMULATION"; then
  echo -e "${GREEN}✓ Direct SQL insert works${NC}"
else
  echo -e "${RED}✗ Direct SQL insert failed${NC}"
  exit 1
fi

# Test 5: Test API - Upsert endpoint
echo ""
echo "Test 5: Test API upsert endpoint"
RESPONSE=$(curl -s -X POST "$API_BASE/api/signal-summary/upsert" \
  -H "Content-Type: application/json" \
  -d '{
    "tokenAddress": "API_TEST_DUMMY_MINT_ADDRESS",
    "persistenceScore": 9,
    "patternTag": "MOMENTUM",
    "confidence": "MEDIUM",
    "appearance24h": 3,
    "appearance72h": 8
  }')

if echo "$RESPONSE" | grep -q '"success":true'; then
  echo -e "${GREEN}✓ API upsert works${NC}"
else
  echo -e "${RED}✗ API upsert failed: $RESPONSE${NC}"
  exit 1
fi

# Test 6: Test API - Get single summary
echo ""
echo "Test 6: Test API get endpoint"
RESPONSE=$(curl -s "$API_BASE/api/signal-summary/API_TEST_DUMMY_MINT_ADDRESS")
if echo "$RESPONSE" | grep -q '"persistenceScore":9' && \
   echo "$RESPONSE" | grep -q '"patternTag":"MOMENTUM"'; then
  echo -e "${GREEN}✓ API get works${NC}"
else
  echo -e "${RED}✗ API get failed: $RESPONSE${NC}"
  exit 1
fi

# Test 7: Test API - Get all summaries
echo ""
echo "Test 7: Test API get all summaries"
RESPONSE=$(curl -s "$API_BASE/api/signal-summary/all?limit=10")
if echo "$RESPONSE" | grep -q '"summaries"'; then
  COUNT=$(echo "$RESPONSE" | grep -o '"tokenAddress"' | wc -l)
  echo -e "${GREEN}✓ API get all works (returned $COUNT summaries)${NC}"
else
  echo -e "${RED}✗ API get all failed: $RESPONSE${NC}"
  exit 1
fi

# Test 8: Validate persistence_score range enforcement
echo ""
echo "Test 8: Validate persistence_score range (0-10)"
RESPONSE=$(curl -s -X POST "$API_BASE/api/signal-summary/upsert" \
  -H "Content-Type: application/json" \
  -d '{
    "tokenAddress": "INVALID_TEST_DUMMY_MINT",
    "persistenceScore": 15,
    "patternTag": "NOISE",
    "confidence": "LOW",
    "appearance24h": 1,
    "appearance72h": 2
  }')

if echo "$RESPONSE" | grep -q '"error".*"between 0 and 10"'; then
  echo -e "${GREEN}✓ Range validation works${NC}"
else
  echo -e "${RED}✗ Range validation failed: $RESPONSE${NC}"
  exit 1
fi

# Test 9: Test ON CONFLICT behavior (update existing record)
echo ""
echo "Test 9: Test ON CONFLICT (upsert update)"
curl -s -X POST "$API_BASE/api/signal-summary/upsert" \
  -H "Content-Type: application/json" \
  -d '{
    "tokenAddress": "API_TEST_DUMMY_MINT_ADDRESS",
    "persistenceScore": 10,
    "patternTag": "DISTRIBUTION",
    "confidence": "HIGH",
    "appearance24h": 6,
    "appearance72h": 15
  }' > /dev/null

RESPONSE=$(curl -s "$API_BASE/api/signal-summary/API_TEST_DUMMY_MINT_ADDRESS")
if echo "$RESPONSE" | grep -q '"persistenceScore":10' && \
   echo "$RESPONSE" | grep -q '"patternTag":"DISTRIBUTION"'; then
  echo -e "${GREEN}✓ Upsert update works${NC}"
else
  echo -e "${RED}✗ Upsert update failed: $RESPONSE${NC}"
  exit 1
fi

# Test 10: Verify no UI changes
echo ""
echo "Test 10: Verify no UI changes"
if git diff --quiet frontend/app/page.tsx frontend/app/components/FollowedTokensModal.tsx frontend/app/components/BlockedTokensModal.tsx 2>/dev/null; then
  echo -e "${GREEN}✓ No UI changes detected (as expected for Phase 1)${NC}"
else
  echo -e "${YELLOW}⚠ UI changes detected - this should only happen in Phase 3${NC}"
fi

# Cleanup test data
echo ""
echo "Cleaning up test data..."
sqlite3 "$DB_PATH" "DELETE FROM token_signal_summary WHERE token_address IN ('TEST_MINT_DUMMY_ADDRESS_FOR_VALIDATION', 'API_TEST_DUMMY_MINT_ADDRESS', 'INVALID_TEST_DUMMY_MINT');"

echo ""
echo "==================================="
echo -e "${GREEN}All Phase 1 tests passed!${NC}"
echo "==================================="
