FROM python:3.13

WORKDIR /app

COPY * .
COPY src/* ./src

RUN apt-get update -y
RUN pip install -r requirements.txt


CMD ["bash"]
