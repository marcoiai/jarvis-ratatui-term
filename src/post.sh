#!/bin/bash

CURL="curl -X POST \
  'https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key=AIzaSyBuoy2mFW-4AI6n3JPDNm-H1eijd5p0EZ8' \
  --header 'Accept: */*' \
  --header 'User-Agent: Thunder Client (https://www.thunderclient.com)' \
  --header 'Content-Type: application/json' \
  --data-raw '{ contents : [{ parts: [{ text: \"$1\"}] }] }' \
  --compressed \
  --no-progress-meter \
  --silent
";

exec bash -c "$CURL";
