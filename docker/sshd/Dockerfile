FROM alpine:3.21
RUN apk add --no-cache openssh-server && \
    ssh-keygen -A && \
    adduser -D -s /bin/sh terminus && \
    echo 'terminus:terminus' | chpasswd && \
    printf '%s\n' \
      'PasswordAuthentication yes' \
      'KbdInteractiveAuthentication yes' \
      'PermitRootLogin no' \
      'AllowTcpForwarding yes' \
      'Subsystem sftp /usr/lib/ssh/sftp-server' \
      >> /etc/ssh/sshd_config
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
