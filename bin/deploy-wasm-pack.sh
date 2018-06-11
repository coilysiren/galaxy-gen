#!/usr/bin/env bash

# setup git
bash bin/travis-git-setup.sh

# setup deploy branch
git pull origin
git checkout deploy
git pull origin deploy

# do main work
make build-wasm

# commit and push changes
git add .
git commit -m "[[ BOT ]] wasm build :: ${TRAVIS_BUILD_NUMBER}"
git pull origin deploy
git push origin deploy:deploy
