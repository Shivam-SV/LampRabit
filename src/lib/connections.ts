import type { ServiceKind } from "./api";

/** How an app connects to a service: a URL, and the same as .env lines. */
export interface Connection {
  url: string;
  env: [string, string][];
}

// 127.0.0.1 rather than localhost: MySQL clients (PHP's included) read "localhost" as
// "use the Unix socket", which isn't where LampRabit's servers listen.
const HOST = "127.0.0.1";

export function connection(kind: ServiceKind, port: number): Connection {
  switch (kind) {
    case "apache":
    case "nginx":
    case "caddy":
    case "frankenphp": {
      const url = `http://localhost${port === 80 ? "" : `:${port}`}`;
      return { url, env: [["APP_URL", url]] };
    }
    case "mongodb":
      return {
        url: `mongodb://${HOST}:${port}`,
        env: [
          ["DB_CONNECTION", "mongodb"],
          ["MONGODB_URI", `mongodb://${HOST}:${port}`],
        ],
      };
    case "clickhouse":
      return {
        url: `http://${HOST}:${port}`,
        env: [
          ["CLICKHOUSE_HOST", HOST],
          ["CLICKHOUSE_PORT", String(port)],
          ["CLICKHOUSE_USER", "default"],
          ["CLICKHOUSE_PASSWORD", ""],
        ],
      };
    case "meilisearch":
      return { url: `http://${HOST}:${port}`, env: [["MEILISEARCH_HOST", `http://${HOST}:${port}`], ["MEILISEARCH_KEY", ""]] };
    case "typesense":
      return {
        url: `http://${HOST}:${port}`,
        env: [
          ["TYPESENSE_HOST", HOST],
          ["TYPESENSE_PORT", String(port)],
          ["TYPESENSE_PROTOCOL", "http"],
          ["TYPESENSE_API_KEY", "lamprabit"],
        ],
      };
    case "rustfs":
      return {
        url: `http://${HOST}:${port}`,
        env: [
          ["AWS_ENDPOINT", `http://${HOST}:${port}`],
          ["AWS_ACCESS_KEY_ID", "lamprabit"],
          ["AWS_SECRET_ACCESS_KEY", "lamprabit-secret"],
          ["AWS_DEFAULT_REGION", "us-east-1"],
          ["AWS_USE_PATH_STYLE_ENDPOINT", "true"],
        ],
      };
    case "minio":
      return {
        url: `http://${HOST}:${port}`,
        env: [
          ["AWS_ENDPOINT", `http://${HOST}:${port}`],
          ["AWS_ACCESS_KEY_ID", "lamprabit"],
          ["AWS_SECRET_ACCESS_KEY", "lamprabit-secret"],
          ["AWS_DEFAULT_REGION", "us-east-1"],
          ["AWS_USE_PATH_STYLE_ENDPOINT", "true"],
        ],
      };
    case "elasticsearch":
      return { url: `http://${HOST}:${port}`, env: [["ELASTICSEARCH_HOST", `http://${HOST}:${port}`]] };
    case "opensearch":
      return { url: `http://${HOST}:${port}`, env: [["OPENSEARCH_HOST", `http://${HOST}:${port}`]] };
    case "n8n":
      return { url: `http://${HOST}:${port}`, env: [["N8N_URL", `http://${HOST}:${port}`]] };
    case "rabbitmq":
      return {
        url: `amqp://guest:guest@${HOST}:${port}`,
        env: [
          ["QUEUE_CONNECTION", "rabbitmq"],
          ["RABBITMQ_HOST", HOST],
          ["RABBITMQ_PORT", String(port)],
          ["RABBITMQ_USER", "guest"],
          ["RABBITMQ_PASSWORD", "guest"],
        ],
      };
    case "ollama":
      return { url: `http://${HOST}:${port}`, env: [["OLLAMA_HOST", `http://${HOST}:${port}`]] };
    case "neo4j":
      return { url: `http://${HOST}:${port}`, env: [["NEO4J_HTTP_URL", `http://${HOST}:${port}`]] };
    case "pgadmin":
      return { url: `http://${HOST}:${port}`, env: [["PGADMIN_URL", `http://${HOST}:${port}`]] };
    case "tomcat":
      return { url: `http://${HOST}:${port}`, env: [["TOMCAT_URL", `http://${HOST}:${port}`]] };
    case "mariadb":
    case "mysql":
      return {
        url: `${kind}://root@${HOST}:${port}`,
        env: [
          ["DB_CONNECTION", kind],
          ["DB_HOST", HOST],
          ["DB_PORT", String(port)],
          ["DB_USERNAME", "root"],
          ["DB_PASSWORD", ""],
        ],
      };
    case "postgres":
      return {
        url: `postgresql://root@${HOST}:${port}/postgres`,
        env: [
          ["DB_CONNECTION", "pgsql"],
          ["DB_HOST", HOST],
          ["DB_PORT", String(port)],
          ["DB_USERNAME", "root"],
          ["DB_PASSWORD", ""],
        ],
      };
    case "redis":
      return {
        url: `redis://${HOST}:${port}`,
        env: [
          ["REDIS_HOST", HOST],
          ["REDIS_PASSWORD", "null"],
          ["REDIS_PORT", String(port)],
        ],
      };
    case "memcached":
      return {
        url: `${HOST}:${port}`,
        env: [
          ["MEMCACHED_HOST", HOST],
          ["MEMCACHED_PORT", String(port)],
        ],
      };
    case "mailpit":
      return {
        url: `smtp://${HOST}:${port}`,
        env: [
          ["MAIL_MAILER", "smtp"],
          ["MAIL_HOST", HOST],
          ["MAIL_PORT", String(port)],
          ["MAIL_USERNAME", "null"],
          ["MAIL_PASSWORD", "null"],
          ["MAIL_ENCRYPTION", "null"],
        ],
      };
  }
}

export const envText = (env: [string, string][]) => env.map(([k, v]) => `${k}=${v}`).join("\n");

/** The text copied for a connection in the given mode. */
export const copyText = (c: Connection, mode: "url" | "env") => (mode === "url" || c.env.length === 0 ? c.url : envText(c.env));
