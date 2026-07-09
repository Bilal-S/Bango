#!/bin/bash

echo "🔍 Scanning current branch for invalid Windows filenames..."

# Regular expression matching forbidden Windows characters
# < > : " \ | ? *
INVALID_CHARS='[<>:"|?*\\]'

# Get a list of all currently tracked files in this branch
# We use -z to handle files with spaces or special characters safely
BAD_FILES=$(git ls-files | grep -E "$INVALID_CHARS")

if [ -z "$BAD_FILES" ]; then
    echo "✅ Success: No invalid Windows filenames detected."
    exit 0
else
    echo "❌ ERROR: Found files containing characters that will break Windows systems:"
    echo "----------------------------------------------------------------------"
    
    # Print the offending files in red for better visibility
    echo -e "\033[31m$BAD_FILES\033[0m"
    
    echo "----------------------------------------------------------------------"
    echo "Please rename or remove these files to maintain Windows compatibility."
    
    # Exit with a non-zero status to fail the CI/CD pipeline or Git hook
    exit 1
fi