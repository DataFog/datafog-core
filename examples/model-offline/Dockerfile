# Prepare wheels/, bundle/ and session.json explicitly before building this image.
# Supply the approved pinned Python base-image digest through --build-arg.
ARG PYTHON_IMAGE
FROM ${PYTHON_IMAGE}
COPY wheels/ /tmp/wheels/
RUN python -m pip install --no-index --find-links=/tmp/wheels \
    datafog-core==0.4.5b1 datafog-gliner2-worker==0.4.5b1
COPY bundle/ /opt/datafog/bundle/
COPY session.json /opt/datafog/session.json
ENV HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 HF_HUB_DISABLE_TELEMETRY=1
CMD ["python", "-c", "import json; from datafog_core import ModelSession; config=json.load(open('/opt/datafog/session.json')); session=ModelSession.open(config); session.warm_up(); print(session.inspect()); session.close()"]
