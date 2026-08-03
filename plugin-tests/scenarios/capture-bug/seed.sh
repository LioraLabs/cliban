#!/usr/bin/env bash
set -eu
DB="$1"
cliban --db "$DB" project add ACME --name "Acme" --description "test fixture project"
