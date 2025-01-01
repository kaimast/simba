#!/bin/bash

for file in shaders/*; do
  if [ -f "$file" ]; then
      echo "Validating $file"
      naga $file
  fi
done
