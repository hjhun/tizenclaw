FROM alpine:latest

# Install essential runtime tools for TizenClaw Skills
# - python3 and py3-pip for Python skill execution
# - nodejs and npm for JS skill execution
# - bash and curl for basic shell operations
RUN apk update && \
    apk add --no-cache \
    python3 \
    py3-pip \
    nodejs \
    npm \
    bash \
    curl \
    jq

# Setup a clean workspace for skills to run in (will be mounted by runc)
RUN mkdir -p /home/owner/share/tizenclaw/workspace/skills
WORKDIR /home/owner/share/tizenclaw/workspace/skills

# Default command placeholder (runc will override this from config.json)
CMD ["/bin/bash"]
