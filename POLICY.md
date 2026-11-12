# 🛡️ Políticas de Uso, Privacidade e Isenção de Responsabilidade

Este documento estabelece as diretrizes de privacidade, direitos autorais, uso responsável e isenção de responsabilidade para o uso do software **`epub-translator-google`**.

---

## 📑 Sumário

- [1. Política de Privacidade & Soberania de Dados (100% Local-First)](#1-política-de-privacidade--soberania-de-dados-100-local-first)
- [2. Direitos Autorais, Propriedade Intelectual & Uso Justo (Fair Use)](#2-direitos-autorais-propriedade-intelectual--uso-justo-fair-use)
- [3. Diretrizes de Uso e Termos de Serviço dos Provedores de Tradução](#3-diretrizes-de-uso-e-termos-de-serviço-dos-provedores-de-tradução)
- [4. Isenção de Garantias e Limitação de Responsabilidade](#4-isenção-de-garantias-e-limitação-de-responsabilidade)
- [5. Licenciamento](#5-licenciamento)

---

## 1. Política de Privacidade & Soberania de Dados (100% Local-First)

O `epub-translator-google` foi arquitetado segundo o paradigma de computação estritamente local (*Local-First*):

1. **Processamento Estritamente Local:**
   A extração, parsing OCF/XML, reconstrução e empacotamento dos contêineres EPUB ocorrem integralmente na memória e no sistema de arquivos local do usuário.
2. **Banco de Dados de Cache SQLite (`translations_cache.db`):**
   O cache de parágrafos e capítulos traduzidos é gravado exclusivamente no arquivo SQLite local (`translations_cache.db` ou caminho customizado via `--db-cache`). Nenhuma informação de cache é enviada, replicada ou compartilhada com servidores dos autores ou mantenedores do projeto.
3. **Ausência Total de Telemetria e Rastreamento:**
   O software **não contém** telemetria, analytics, rastreadores de atividade, identificadores de usuário, coleta de métricas de uso ou chamadas ocultas para servidores proprietários.
4. **Comunicação de Rede Exclusiva:**
   As únicas comunicações de rede efetuadas pelo programa são as requisições diretas e indispensáveis para os provedores de tradução configurados pelo próprio usuário no ato da execução.

---

## 2. Direitos Autorais, Propriedade Intelectual & Uso Justo (Fair Use)

1. **Finalidade Utilitária e Educacional:**
   O `epub-translator-google` é uma ferramenta de software utilitária, concebida para auxiliar usuários em atividades pessoais, estudos, pesquisas acadêmicas, fins educacionais e promoção de acessibilidade literária e linguística.
2. **Obras Aplicáveis:**
   O uso pretendido da ferramenta abrange:
   - Obras que se encontrem sob **Domínio Público**;
   - Textos e livros sob licenças abertas (como Creative Commons);
   - Obras proprietárias obtidas legalmente pelo próprio usuário para fins estritos de uso pessoal, estudo privado ou acessibilidade, onde a legislação de sua jurisdição autorize o formato alternativo ou a cópia privada (*Fair Use*, *Fair Dealing* ou exceções da Lei de Direitos Autorais).
3. **Responsabilidade Exclusiva do Usuário:**
   O usuário final é o **único e integral responsável** por todo e qualquer arquivo EPUB submetido à tradução, bem como pela observância e conformidade com as legislações de direitos autorais e propriedade intelectual aplicáveis no país de uso (incluindo a Lei nº 9.610/1998 no Brasil, o *Digital Millennium Copyright Act - DMCA* nos EUA, a Convenção de Berna e legislações internacionais pertinentes).
4. **Proibição de Uso Ilícito:**
   É expressamente vedado o uso desta ferramenta para fins de pirataria, distribuição pública não autorizada, compartilhamento em massa ou exploração comercial de obras protegidas por direitos autorais sem a prévia e expressa autorização de seus respectivos autores ou detentores de direitos.
5. **Isenção dos Mantenedores:**
   Os desenvolvedores e mantenedores do `epub-translator-google` não incentivam, não apoiam e não se responsabilizam por eventuais infrações de direitos autorais ou uso indevido praticado por terceiros por meio deste software.

---

## 3. Diretrizes de Uso e Termos de Serviço dos Provedores de Tradução

O software atua como uma ponte cliente para serviços e motores de tradução externos. Cabe ao usuário garantir a conformidade com as políticas e os termos de serviço dos provedores escolhidos:

### A) Google Antigravity (Gemini CLI / `agy`)
- Requer ambiente configurado e autenticado com a CLI do Antigravity / Gemini.
- O usuário deve respeitar as políticas de uso aceitável de Inteligência Artificial do Google e os termos vigentes para a respectiva conta e projeto.

### B) Google Cloud Translation API (`google-cloud`)
- Requer o fornecimento de chave de API própria (`--api-key`).
- O usuário é o titular da conta no Google Cloud Platform (GCP) e responde integralmente pelo gerenciamento de credenciais, faturamento, custos incorridos e cumprimento dos *Google Cloud Platform Terms of Service*.

### C) Google Free Endpoint (`google-free` / `gtx`)
- Endpoint público de conveniência web não autenticado.
- A ferramenta implementa internamente semáforos concorrentes Tokio com limitação de taxa e jitter temporal para evitar sobrecarga de requisições.
- O usuário deve agir com razoabilidade e moderação, ciente de que o serviço pode impor bloqueios temporários de IP (HTTP 429), alterações de contrato ou indisponibilidade a qualquer momento, sem garantia de SLA.

### D) Isenção de Afiliação
- Este projeto de código aberto **não é afiliado, patrocinado, associado ou endossado pela Google LLC**.
- Todas as marcas comerciais, nomes de serviços e logos pertencem aos seus respectivos proprietários.

---

## 4. Isenção de Garantias e Limitação de Responsabilidade

1. Em conformidade com os termos da Licença MIT, o software é fornecido **"NO ESTADO EM QUE SE ENCONTRA" ("AS IS")**, sem qualquer garantia expressa ou implícita, incluindo, mas não se limitando a, garantias de comerciabilidade, adequação a uma finalidade específica e não violação de direitos.
2. Em nenhuma hipótese os autores ou detentores de direitos autorais serão responsáveis por qualquer dano direto, indireto, incidental, especial ou consequencial decorrente do uso ou da incapacidade de uso deste software, incluindo perda de dados, corrupção de arquivos ou custos decorrentes do consumo de APIs pagas.
3. Não há garantia de exatidão sintática, semântica ou literária das traduções geradas, cabendo ao usuário revisar e validar o conteúdo final produzido.

---

## 5. Licenciamento

O código-fonte do projeto está sob a [Licença MIT](LICENSE). Para o texto integral da licença, consulte o arquivo [LICENSE](LICENSE) na raiz do repositório.
