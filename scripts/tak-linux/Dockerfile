FROM ubuntu:24.04

ARG TAK_VERSION=0.0.13
ARG TAK_SHA256=514c890ad2f34d5fbfa7e9327537a3d96a20ba863c9e45e74e210de13fa4b2aa

RUN apt-get update \
	&& apt-get install -y --no-install-recommends ca-certificates curl git valgrind \
	&& rm -rf /var/lib/apt/lists/*

RUN curl -fsSL -o /tmp/tak.tar.gz \
		"https://github.com/jdx/tak/releases/download/v${TAK_VERSION}/tak-aarch64-unknown-linux-gnu.tar.gz" \
	&& echo "${TAK_SHA256}  /tmp/tak.tar.gz" | sha256sum -c - \
	&& tar -xzf /tmp/tak.tar.gz -C /usr/local/bin \
	&& rm /tmp/tak.tar.gz \
	&& tak --version

# The repo is bind-mounted from macOS under a different owner.
RUN git config --system --add safe.directory '*'

WORKDIR /work
