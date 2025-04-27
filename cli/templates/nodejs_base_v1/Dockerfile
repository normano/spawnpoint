# Use an official Node.js runtime as a parent image
# Use a specific LTS version for better reproducibility
FROM node:18-alpine As development

# Set the working directory in the container
WORKDIR /usr/src/app

# Copy package.json and package-lock.json (or yarn.lock)
# Use placeholders that will be substituted by spawnpoint
COPY package.json ./
# If using yarn:
# COPY yarn.lock ./
# If using npm:
COPY package-lock.json ./

# Install app dependencies
# Use --frozen-lockfile (yarn) or ci (npm) for reproducible installs
# RUN yarn install --frozen-lockfile
RUN npm ci --only=production

# Copy app source
COPY . .

# Build the app (if using TypeScript or similar)
# RUN yarn build

# Production image
FROM node:18-alpine As production

ARG NODE_ENV=production
ENV NODE_ENV=${NODE_ENV}

WORKDIR /usr/src/app

COPY package.json ./
# If using yarn:
# COPY yarn.lock ./
# If using npm:
COPY package-lock.json ./

# Install production dependencies only
# RUN yarn install --production --frozen-lockfile
RUN npm ci --only=production

# Copy built app from development stage
# Modify this based on your build output (e.g., dist folder)
COPY --from=development /usr/src/app/dist ./dist
# If not building, copy source:
# COPY --from=development /usr/src/app/src ./src
# COPY --from=development /usr/src/app/index.js ./index.js


# Expose port (use a variable placeholder if needed)
EXPOSE 3000

# Define the command to run your app
# Update 'dist/index.js' to your actual entry point
CMD [ "node", "dist/index.js" ]