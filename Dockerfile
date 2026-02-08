FROM node:lts

RUN npm install -g @anthropic-ai/claude-code

USER node
WORKDIR /home/node

CMD ["claude", "--dangerously-skip-permissions"]
