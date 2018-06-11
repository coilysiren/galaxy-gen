#!/usr/bin/env bash

# setup git
bash ./travis-git-setup.sh

# setup deploy-webpack-log branch
git pull origin
git checkout deploy-webpack-log
git pull origin deploy-webpack-log

# do main work
make build-js-prod

# commit and push changes
git add .
git commit -m "[[ BOT ]] webpack build :: ${TRAVIS_BUILD_NUMBER}"
git pull origin deploy-webpack-log
git push origin deploy-webpack-log:deploy-webpack-log
