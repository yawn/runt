# Run template (`runt`)

[![Rust](https://github.com/yawn/runt/actions/workflows/ci.yml/badge.svg)](https://github.com/yawn/runt/actions/workflows/ci.yml)

`runt` is a simple commandline utility to make configuration files executable. 

## Example

Considering a configuration file `s3.yml` with the executable bit set:

```
#! /usr/local/bin/runt -v
# aws cloudformation deploy
#   --template-file $RUNT_TAIL
#   --stack-name ${1-test}
#   --capabilities CAPABILITY_IAM
#   --parameter-overrides Foo=bar
AWSTemplateFormatVersion: 2010-09-09
Transform: AWS::Serverless-2016-10-31

Parameters:
  Foo:
    Type: String

Resources:
  HelloBucket:
    Type: AWS::S3::Bucket
```

When executing this file, `runt`will

1. open the _file_ it was invoked on as part of the shebang, in this case `s3.yml`
2. collect all lines following the shebang that start with `#` into one line (the _head_) - note that the length of the head is allowed to exceed the length of a shebang line 
3. copy the following lines into a temporary file (the _tail_)

`runt` then provides an env with the following variables added:

* `$RUNT_TAIL` (can be overriden with `-t` or `--env-for-tail`) with the absolute path to the tail
* `$@`, `$0..n` and `$#` are overriden with optional arguments passed to the file with `$0` being set to the name of the file  

This env is then used to `exec`ute the head.

## Configuration options

`runt` has very few configuration options:

* `-t, --env-for-tail` (default: `$RUNT_TAIL`) specifies the tail location
* `-k, --keep-tail` will flag the temporary tail file as "do not delete"
* `-v, --verbose` will output some debug information while running
