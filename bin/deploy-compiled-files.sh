#!/usr/bin/env bash

# setup git
git init
git config --global user.email "lynncyrin@gmail.com"
git config --global user.name "Lynn Cyrin"
git config --global pull.rebase true

# remove travis's readonly origin, add our origin with write permissions
# NOTE: GITHUB_API_TOKEN needs to be added to travis
# https://blog.github.com/2013-05-16-personal-api-tokens/
# https://docs.travis-ci.com/user/environment-variables/#Defining-Variables-in-Repository-Settings
git remote remove origin
git remote add origin https://${GITHUB_API_TOKEN}@github.com/${TRAVIS_REPO_SLUG}.git

# setup our branch
git pull origin
git checkout deploy
git pull origin deploy
git pull origin main

# do main work
make build-wasm
make build-js-prod

# commit and push changes
git add .
git commit -m "[[ BOT ]] build :: ${TRAVIS_BUILD_NUMBER}"
git pull origin deploy
git push origin HEAD
