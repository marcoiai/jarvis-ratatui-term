#!/bin/bash

CURL="curl -X POST \
  'https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key=$2' \
  --header 'Accept: */*' \
  --header 'User-Agent: Thunder Client (https://www.thunderclient.com)' \
  --header 'Content-Type: application/json' \
  --data-raw '{ contents : [{ parts: [{ text: \"$1\"}] }] }' \
  --compressed \
  --no-progress-meter \
  --silent
";

exec bash -c "$CURL";
