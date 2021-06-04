# Drone gitea release plugin

A very simple plugin to create a release on gitea and publish files to it. Built with the idea that pipelines will have a pre-processing step, so checksum generation, notes generation, and so on, is assumed to just be done via existing files.

## Settings

```yaml
base_url: https://try.gitea.io
api_key:
  from_secret: api_key
name: title.txt
body: notes.txt
draft: false
prerelease: false
assets:
  - release.tar.gz
  - release.tar.gz.sha256
  - release.tar.gz.sha512
```
