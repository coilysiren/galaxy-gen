#!/usr/bin/env bash

# setup git
bash ./travis-git-setup.sh

# setup deploy branch
git pull origin
git checkout deploy
git pull origin deploy

# do main work
make build-js-prod

# commit and push changes
git add .
git commit -m "[[ BOT ]] webpack build :: ${TRAVIS_BUILD_NUMBER}"
git pull origin deploy
git push origin deploy:deploy
