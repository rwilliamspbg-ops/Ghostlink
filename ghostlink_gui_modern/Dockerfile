FROM node:18-alpine as build
WORKDIR /app
COPY package*.json ./
RUN npm ci --legacy-peer-deps
COPY . .
RUN npm run build

FROM node:18-alpine
WORKDIR /app
RUN npm install -g serve
COPY --from=build /app/dist ./dist
# Add entrypoint script to inject env vars into window._env_ if needed
# but for now we rely on VITE_API_BASE at build time or
# we can use a simple script to replace it in the dist files.

ENV GHOSTLINK_API_BASE=http://127.0.0.1:8003
EXPOSE 5173

# Script to inject environment variables at runtime
RUN printf '#!/bin/sh\necho "window._env_ = { GHOSTLINK_API_BASE: \"$GHOSTLINK_API_BASE\" };" > /app/dist/env-config.js\nexec serve -s dist -l 5173\n' > /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENTRYPOINT ["/app/entrypoint.sh"]
