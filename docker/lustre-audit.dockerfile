FROM imlteam/manager-service-base:5.1

CMD ["python", "./manage.py", "chroma_service", "--name=lustre_audit", "lustre_audit", "--console"]