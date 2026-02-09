FROM node:lts

RUN npm install -g @anthropic-ai/claude-code

USER node
WORKDIR /home/node

RUN git config --global user.name "Claude" \
 && git config --global user.email "noreply@anthropic.com" \
 && git config --global credential.helper "store --file ~/.git-credentials" \
 && git config --global credential.useHttpPath true

CMD ["claude", "--dangerously-skip-permissions"]

