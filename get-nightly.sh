#!/bin/bash

date=`date +%Y-%m-%d`
tar=rust-nightly-${date}.tar.gz

wget -O $tar http://static.rust-lang.org/dist/rust-nightly-x86_64-unknown-linux-gnu.tar.gz
tar xf $tar --transform "s/rust-nightly-x86_64-unknown-linux-gnu/${date}/"
