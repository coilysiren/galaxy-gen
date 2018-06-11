#!/usr/bin/env bash

# setup git
git init
git config --global user.email "lynncyrin@gmail.com"
git config --global user.name "[[ BOT ]] Lynn Cyrin"
git remote remove origin
git remote add origin https://${GITHUB_API_TOKEN}@github.com/${TRAVIS_REPO_SLUG}.git
git pull origin

# setup deploy branch
git checkout deploy
git merge -X theirs main --allow-unrelated-histories --no-commit --no-ff
git commit --no-edit -m '[[ BOT ]] merge main => deploy'
git pull

# do main work
make build-wasm

# commit and push changes
git add .
git commit -m "[[ BOT ]] wasm build :: ${TRAVIS_BUILD_NUMBER}"
git push origin HEAD:deploy
