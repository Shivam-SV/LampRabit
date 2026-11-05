import type { EnvPaths, Runtime, Server, Settings } from "./api";

type Os = EnvPaths["os"];
export type PortKey = keyof Settings["ports"];

/**
 * Every configurable port. `only` limits a field to the platforms where it does anything;
 * `server` to while that web server runs next to the primary one (`admin`: whenever it's
 * enabled). `runtime` is the service whose details show the field on the Services page.
 */
export const portFields: { key: PortKey; label: string; hint: string; runtime?: Runtime; only?: Os[]; server?: Server; admin?: boolean }[] = [
  { key: "http", label: "HTTP", hint: "Primary web server" },
  { key: "https", label: "HTTPS", hint: "Primary web server, SSL" },
  { key: "apacheHttp", label: "Apache HTTP", hint: "Apache next to the primary server", server: "apache" },
  { key: "apacheHttps", label: "Apache HTTPS", hint: "Apache next to the primary server", server: "apache" },
  { key: "nginxHttp", label: "Nginx HTTP", hint: "Nginx next to the primary server", server: "nginx" },
  { key: "nginxHttps", label: "Nginx HTTPS", hint: "Nginx next to the primary server", server: "nginx" },
  { key: "caddyHttp", label: "Caddy HTTP", hint: "Caddy next to the primary server", server: "caddy" },
  { key: "caddyHttps", label: "Caddy HTTPS", hint: "Caddy next to the primary server", server: "caddy" },
  { key: "caddyAdmin", label: "Caddy admin", hint: "Local API for reloads, 127.0.0.1 only", server: "caddy", admin: true },
  { key: "frankenphpHttp", label: "FrankenPHP HTTP", hint: "FrankenPHP next to the primary server", server: "frankenphp" },
  { key: "frankenphpHttps", label: "FrankenPHP HTTPS", hint: "FrankenPHP next to the primary server", server: "frankenphp" },
  { key: "frankenphpAdmin", label: "FrankenPHP admin", hint: "Local API for reloads, 127.0.0.1 only", server: "frankenphp", admin: true },
  { key: "tomcatHttp", label: "Tomcat HTTP", hint: "Java web apps, behind the web server", runtime: "tomcat" },
  { key: "tomcatShutdown", label: "Tomcat shutdown", hint: "Tomcat's stop command, 127.0.0.1 only", runtime: "tomcat" },
  { key: "mysql", label: "MariaDB", hint: "Database", runtime: "mariadb" },
  { key: "mysqlServer", label: "MySQL", hint: "Database, next to MariaDB", runtime: "mysql" },
  { key: "postgres", label: "PostgreSQL", hint: "Database", runtime: "postgres" },
  { key: "pgadmin", label: "pgAdmin", hint: "Its web server, behind pgadmin.test", runtime: "pgadmin" },
  { key: "mongodb", label: "MongoDB", hint: "Document database", runtime: "mongodb" },
  { key: "clickhouseHttp", label: "ClickHouse HTTP", hint: "Queries over HTTP and the /play console", runtime: "clickhouse" },
  { key: "clickhouseTcp", label: "ClickHouse native", hint: "clickhouse-client and native drivers", runtime: "clickhouse" },
  { key: "neo4jHttp", label: "Neo4j Browser", hint: "Its web interface, behind neo4j.test", runtime: "neo4j" },
  { key: "neo4jBolt", label: "Neo4j Bolt", hint: "What drivers connect to", runtime: "neo4j" },
  { key: "meilisearch", label: "Meilisearch", hint: "Its API and search preview", runtime: "meilisearch" },
  { key: "typesense", label: "Typesense", hint: "Its API", runtime: "typesense" },
  { key: "rustfs", label: "RustFS S3", hint: "The S3 API apps connect to", runtime: "rustfs" },
  { key: "rustfsConsole", label: "RustFS console", hint: "Its web console", runtime: "rustfs" },
  { key: "ollama", label: "Ollama", hint: "Its API", runtime: "ollama" },
  { key: "minio", label: "MinIO S3", hint: "The S3 API apps connect to", runtime: "minio" },
  { key: "minioConsole", label: "MinIO console", hint: "Its web console", runtime: "minio" },
  { key: "elasticsearch", label: "Elasticsearch", hint: "Its HTTP API", runtime: "elasticsearch" },
  { key: "elasticsearchTransport", label: "Elasticsearch transport", hint: "Node-to-node, 127.0.0.1 only", runtime: "elasticsearch" },
  { key: "opensearch", label: "OpenSearch", hint: "Its HTTP API", runtime: "opensearch" },
  { key: "opensearchTransport", label: "OpenSearch transport", hint: "Node-to-node, 127.0.0.1 only", runtime: "opensearch" },
  { key: "n8n", label: "n8n", hint: "Its editor, behind n8n.test", runtime: "n8n" },
  { key: "rabbitmq", label: "RabbitMQ", hint: "AMQP, what apps connect to", runtime: "rabbitmq" },
  { key: "rabbitmqManagement", label: "RabbitMQ console", hint: "Its management console", runtime: "rabbitmq" },
  { key: "phpFpm", label: "PHP FastCGI", hint: "First of 10 ports per PHP version", runtime: "php" },
  { key: "dumps", label: "Dumps", hint: "Where dump() sends dumps for Logs, while catching is on", runtime: "php" },
  { key: "redis", label: "Redis", hint: "Cache and queues", runtime: "redis" },
  { key: "memcached", label: "Memcached", hint: "Memory cache", runtime: "memcached" },
];

/** Mailpit's ports, shown in Settings → Email rather than the port grid. */
export const mailPortFields: { key: PortKey; label: string; hint: string }[] = [
  { key: "mailSmtp", label: "SMTP port", hint: "Where apps send mail" },
  { key: "mailUi", label: "Inbox port", hint: "The web inbox" },
];

export const serverNotes: Record<Server, string> = {
  apache: "Honours .htaccess files, like XAMPP and Laragon.",
  nginx: "Ignores .htaccess; unknown URLs go to index.php, which suits Laravel, Symfony and WordPress.",
  caddy: "Ignores .htaccess; unknown URLs go to index.php. Its config is short and readable (etc/caddy/Caddyfile).",
  frankenphp: "Caddy with its own PHP built in: fast, but LampRabit's PHP versions and extensions don't apply to its sites. Ignores .htaccess.",
};

/** Whether a port field applies with these settings (platform, enabled servers). */
export const portShown = (f: (typeof portFields)[number], settings: Settings, os?: Os) =>
  (!f.only || (!!os && f.only.includes(os))) &&
  (!f.server || (settings.webServers.includes(f.server) && (f.admin || settings.webServers[0] !== f.server)));

/** The ports a service's details show: a web server's own pair (the main one while it's primary), others theirs. */
export function portsFor(runtime: Runtime, settings: Settings, os?: Os) {
  if (runtime === "mailpit") return mailPortFields;
  const server = runtime as Server;
  if ((["apache", "nginx", "caddy", "frankenphp"] as Runtime[]).includes(runtime)) {
    const primary = settings.webServers[0] === server;
    return portFields.filter((f) => (f.server === server && (f.admin || !primary)) || (primary && !f.server && !f.runtime));
  }
  return portFields.filter((f) => f.runtime === runtime && portShown(f, settings, os));
}
