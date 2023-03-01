# OpenTelemetry config

At the moment this isn't *great* but it works.

Running jaeger locally on docker (from [the quickstart docs](https://www.jaegertracing.io/docs/1.42/getting-started/))

```bash
docker run --rm -it --name jaeger \
  -e COLLECTOR_ZIPKIN_HOST_PORT=:9411 \
  -e COLLECTOR_OTLP_ENABLED=true \
  -p 6831:6831/udp \
  -p 6832:6832/udp \
  -p 5778:5778 \
  -p 16686:16686 \
  -p 4317:4317 \
  -p 4318:4318 \
  -p 14250:14250 \
  -p 14268:14268 \
  -p 14269:14269 \
  -p 9411:9411 \
  jaegertracing/all-in-one:latest
```


Example environment variables:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4317" #grpc
export OTEL_SERVICE_NAME="goatns"
export OTEL_LOG_LEVEL="debug"
export OTEL_TRACES_SAMPLER="always_on"
export OTEL_EXPORTER_OTLP_TRACES_PROTOCOL="grpc"
```
