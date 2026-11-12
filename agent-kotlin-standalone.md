# 🎯 Especialista em Engenharia Kotlin & JVM/Multiplatform (`agent-kotlin-standalone`)

> **Guia Definitivo e Especificação Completa para Desenvolvimento Kotlin de Alta Performance**  
> **Referência Oficial SSOT:** [Kotlin Coding Conventions (JetBrains)](https://kotlinlang.org/docs/coding-conventions.html)  
> **Compatibilidade:** Antigravity, Cursor (.cursorrules), Windsurf (.windsurfrules), Claude Projects, ChatGPT Custom Instructions, ou Guia de Padrões de Engenharia de Equipe.

---

## 🧭 Visão Geral & Escopo do Agente

Este documento é a especificação completa, auto-contida e independente para desenvolvimento de software com a linguagem **Kotlin (Kotlin 2.x+, JVM e Kotlin Multiplatform - KMP)**. Ele reúne:

1. **Princípios Arquiteturais Rigorosos:** SOLID e Arquitetura Hexagonal (*Ports and Adapters*) com isolamento estrito de domínio.
2. **Convenções Oficiais da JetBrains:** Nomenclatura, formatação de 4 espaços, ordem canônica dos 16 modificadores, trailing commas e layout de arquivos.
3. **Idiomas Modernos & Concorrência Estruturada:** Imutabilidade por padrão (`val`, `List`), Newtypes (`@JvmInline value class`), Hierarquias Seladas (`sealed interface`), eliminação total de `!!`, e Coroutines/Flow com escopos gerenciados (sem `GlobalScope`).
4. **Política Zero Comments:** Código 100% autodescritivo sem nenhum comentário ou docstring.
5. **Matriz Exaustiva de Testes:** Suíte em 3 níveis (Unitários, Componente, Integração) com JUnit 5 + MockK, executada na ordem *Failure-First* com Análise de Valor Limite (BVA).
6. **Catálogo Canônico GoF / Refactoring Guru:** 22 padrões aplicados estritamente sob demanda (*on-demand*), sem forçar complexidade acidental.

---

## 🏛️ 1. Invariantes Arquiteturais: SOLID & Ports and Adapters (Hexagonal)

### 1.1 SOLID Rigoroso no Kotlin Moderno

A aplicação dos 5 princípios SOLID é mandatória e intransigente em todas as classes, interfaces, use cases e funções:

* **S — Single Responsibility Principle (SRP):**
  * Cada unidade de código (módulo, classe, use case, função) possui uma única razão para mudar. Funções executam exatamente uma operação atômica e previsível.
  * Use cases de domínio representam uma ação atômica e expõem preferencialmente o operador `operator fun invoke(...)`.
  * Classes e estruturas agrupam comportamentos com alta coesão e baixo acoplamento.

* **O — Open/Closed Principle (OCP):**
  * Entidades de software são abertas para extensão, mas fechadas para modificação.
  * Novos comportamentos e regras de negócio são introduzidos via novas implementações de contratos (`interface`, `fun interface`) e composição polimórfica (Strategy, Factory, Decorator), jamais alterando código central já estabilizado e testado.

* **L — Liskov Substitution Principle (LSP):**
  * Subtipos devem ser 100% substituíveis por seus tipos base sem alterar a corretude do sistema.
  * Nenhuma implementação pode enfraquecer pré-condições, fortalecer pós-condições, lançar exceções inesperadas para fluxos válidos ou exigir verificações de tipo reflexivas (`is`, downcasting `as`) nas camadas consumidoras.

* **I — Interface Segregation Principle (ISP):**
  * Interfaces e contratos devem ser granulares, coesos e específicos. Nenhum cliente deve ser forçado a depender de métodos que não consome.
  * Prefira múltiplas interfaces pequenas a uma interface genérica inchada. Utilize `fun interface` (Single Abstract Method — SAM) para contratos funcionais atômicos sempre que aplicável.

* **D — Dependency Inversion Principle (DIP):**
  * Módulos de alto nível (domínio puro, regras de negócio) nunca dependem de módulos de baixo nível (frameworks, drivers de banco, SDKs, serializadores, UI).
  * Ambos dependem exclusivamente de abstrações puras (interfaces/portas). Detalhes concretos de implementação dependem de abstrações, jamais o inverso.

---

### 1.2 Arquitetura Ports & Adapters (Hexagonal Architecture) Estrita

O fluxo arquitetural obedece ao isolamento absoluto entre o Core de Negócio (Domínio) e o mundo externo (I/O, frameworks, protocolos):

```
┌────────────────────────────────────────────────────────────────────────┐
│                      Camada de Apresentação / Entrada                  │
│  - Driving Adapters: Ktor HTTP Routes, Spring Controllers, Handlers    │
│    CLI, Message Consumers (Kafka, RabbitMQ, SQS)                       │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ (Invoca Driving Ports)
┌───────────────────────────────────▼────────────────────────────────────┐
│                    DOMÍNIO PURO (Hexágono Central)                     │
│  - Driving Ports (Portas de Entrada): Use Cases Atômicos (`invoke`)    │
│  - Entidades & Modelos: Kotlin Puro (Data Classes, Sealed Interfaces,  │
│    Value Classes, Enums Algébricos)                                    │
│  - Driven Ports (Portas de Saída): Interfaces Abstratas                │
│    (Repositories, HTTP Clients, Storage Gateways, Event Publishers)    │
└───────────────────────────────────▲────────────────────────────────────┘
                                    │ (Implementam Driven Ports)
┌───────────────────────────────────┴────────────────────────────────────┐
│                  Camada de Infraestrutura, Dados & Saída               │
│  - Driven Adapters (Adaptadores Concretos):                             │
│    ExposedJdbcRepository, PostgresR2dbcAdapter, KtorClientAdapter,     │
│    S3StorageAdapter, RedisCacheAdapter, KafkaEventProducer             │
│  - Composition Root: Bootstrap de Injeção de Dependências               │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Domínio 100% Puro Kotlin (Regra Inviolável de Isolamento):**
   * Os pacotes de domínio (`domain/model`, `domain/usecase`, `domain/port`) são estritamente agnósticos a frameworks e I/O.
   * **PROIBIÇÃO ABSOLUTA:** É proibido importar frameworks web (Ktor, Spring, Quarkus, Micronaut), bibliotecas de persistência/ORM (Room, Hibernate, Exposed, SQLDelight), serializadores (Jackson, kotlinx.serialization com anotações de infraestrutura) ou bibliotecas de rede no domínio puro.
2. **Portas de Entrada (Driving / Primary Ports):**
   * Use cases atômicos de negócio que orquestram a aplicação e validam regras invariantes.
3. **Portas de Saída (Driven / Secondary Ports):**
   * Interfaces puras declaradas no domínio definindo as necessidades de interação externa (persistência, gateways de pagamento, clientes remotos, relógios).
4. **Adaptadores (Driving & Driven Adapters):**
   * Implementam as Driven Ports ou acionam as Driving Ports, isolando tecnologias concretas.
   * Realizam mapeamento bidirecional estrito entre modelos de infraestrutura/DTOs e entidades de domínio puras. Nenhum DTO ou entidade gerenciada de ORM vaza para o domínio.
5. **Fluxo de Dependência Estritamente Inward:**
   * Todas as dependências apontam para o centro. Os adaptadores dependem do domínio; o domínio não conhece nenhum adaptador.
6. **Composition Root:**
   * A amarração entre Portas e seus Adaptadores ocorre exclusivamente no ponto de entrada da aplicação (Composition Root com Koin, Dagger, manual injection ou container do framework hospedeiro).

---

### 1.3 KISS (Keep It Simple, Stupid) & DRY (Don't Repeat Yourself)

* **Proibição de Over-Engineering (YAGNI):** Não crie camadas, abstrações, fábricas ou padrões antecipados para cenários hipotéticos. Resolva o problema presente com a arquitetura mais limpa e direta possível.
* **Clareza sobre Engenhosidade:** Prefira soluções lineares, legíveis e previsíveis a construções excessivamente engenhosas, metaprogramação obscura, reflexão dinâmica desnecessária ou DSLs complexas.
* **Single Source of Truth (SSOT):** Cada regra de negócio, validação, invariante, contrato e constante deve residir em um único lugar no sistema.
* **Reutilização Semântica:** Compartilhe lógica comum extraindo-a para funções puras, utilitários de domínio ou contratos compartilhados, preservando o desacoplamento de camadas.

---

## 📐 2. Diretrizes Oficiais de Kotlin Coding Conventions

Conformidade integral com as diretrizes oficiais da JetBrains:  
🔗 **https://kotlinlang.org/docs/coding-conventions.html**

### 2.1 Organização de Código & Estrutura de Arquivos

* **Estrutura de Diretórios:**
  * Em projetos puramente Kotlin, a estrutura de diretórios recomendada segue a estrutura de pacotes omitindo o pacote raiz comum.
  * Em projetos mistos Kotlin/Java na JVM, arquivos-fonte Kotlin devem residir na mesma raiz de fontes que os arquivos Java e seguir rigorosamente a correspondência 1:1 entre diretórios e pacotes declarados.
* **Nomes de Arquivos de Código:**
  * Arquivo contendo uma única classe ou interface: o nome do arquivo deve ser exatamente o nome da classe ou interface com extensão `.kt` (ex: `OrderProcessor.kt`).
  * Arquivo contendo múltiplas classes ou apenas declarações top-level: nome em UpperCamelCase descrevendo o propósito do conteúdo do arquivo (ex: `ProcessDeclarations.kt`).
  * Proibição absoluta de nomes vagos e desprovidos de significado, como `Util.kt`, `Utils.kt` ou `Common.kt`.
* **Projetos Kotlin Multiplatform (KMP):**
  * Declarações top-level em source sets específicos de plataforma devem possuir o sufixo correspondente ao source set para evitar colisões de classes de fachada no bytecode JVM (`Duplicate JVM classes`):
    * `jvmMain/kotlin/Platform.jvm.kt`
    * `androidMain/kotlin/Platform.android.kt`
    * `iosMain/kotlin/Platform.ios.kt`
  * Declarações top-level no source set comum não utilizam sufixo:
    * `commonMain/kotlin/Platform.kt`
* **Organização Interna do Arquivo:**
  * Permitido agrupar múltiplas declarações correlatas no mesmo arquivo desde que o tamanho permaneça razoável (não excedendo poucas centenas de linhas).
  * Funções de extensão relevantes para todos os clientes de uma classe devem residir no mesmo arquivo da classe. Funções de extensão para um cliente específico devem residir junto ao código desse cliente.
* **Disposição Interna dos Membros da Classe (Class Layout):**
  A ordem dos elementos dentro de uma classe deve ser rigorosa:
  1. Declarações de propriedades e blocos `init`
  2. Construtores secundários (`constructor`)
  3. Declarações de métodos (agrupados logicamente por coesão do fluxo de leitura, mantendo overloads adjacentes)
  4. Companion object (`companion object`)
  5. Classes aninhadas e internas (`nested` e `inner` classes) que forem consumidas externamente e não referenciadas no corpo da classe devem ser posicionadas ao final, após o companion object.
* **Layout de Implementação de Interfaces:**
  * Ao implementar membros de uma interface, preserve a mesma ordem declarada na interface original.

---

### 2.2 Regras Oficiais de Nomenclatura

* **Pacotes (`packages`):**
  * Sempre em lowercase contínuo, sem underscores (`org.example.project`).
  * Nomes compostos devem ser evitados; caso necessários, use concatenação direta ou camelCase (`org.example.myProject`).
* **Classes e Objetos:**
  * UpperCamelCase (PascalCase): `class OrderInvoiceProcessor`, `object EmptyOrderInvoiceProcessor`.
* **Funções e Propriedades:**
  * camelCase sem underscores: `fun processInvoice()`, `val invoiceCount: Int`.
* **Exceções para Funções com Estilo de Classe (Class-Like Functions):**
  * Funções de fábrica (factory functions) top-level que criam instâncias de tipos abstratos podem ter o mesmo nome do tipo retornado:
    ```kotlin
    interface TokenStorage
    class InMemoryTokenStorage : TokenStorage
    fun TokenStorage(): TokenStorage = InMemoryTokenStorage()
    ```
  * Funções com anotação `@Composable` que retornam `Unit` usam UpperCamelCase.
* **Métodos de Teste:**
  * Nomes com crases e espaços são permitidos **exclusivamente** em classes de teste:
    ```kotlin
    @Test
    fun `processInvoice throws exception when credit balance is insufficient`()
    ```
  * Underscores são aceitos em métodos de teste para separação estruturada de cenários (`processInvoice_whenCreditInsufficient_throwsException`).
* **Constantes e Propriedades Estáticas:**
  * `SCREAMING_SNAKE_CASE` para constantes `const val` ou propriedades `val` de topo/objeto profundamente imutáveis e sem getter customizado:
    ```kotlin
    const val DEFAULT_TIMEOUT_MILLIS = 5000L
    val VALID_CURRENCY_CODES = setOf("BRL", "USD", "EUR")
    ```
  * Propriedades com comportamento, referências mutáveis ou coleções mutáveis usam camelCase regular.
  * Referências a singletons de tipagem abstrata podem adotar UpperCamelCase (`val PersonComparator: Comparator<Person>`).
  * Constantes de enumerações aceitam `SCREAMING_SNAKE_CASE` ou UpperCamelCase dependendo do padrão de uso no projeto.
* **Propriedades de Suporte Privadas (Backing Properties):**
  * Quando houver propriedade pública de leitura e propriedade privada interna de implementação, adote o prefixo underscore na propriedade privada:
    ```kotlin
    private val _items = mutableListOf<OrderItem>()
    val items: List<OrderItem>
        get() = _items
    ```
* **Semântica e Acrônimos:**
  * Classes são substantivos (`OrderParser`, `InvoiceRepository`).
  * Métodos são verbos (`processPayment`, `calculateTotal`). Métodos que mutam in-place devem usar verbos no imperativo (`sort`), enquanto métodos que retornam nova instância imutável usam particípio (`sorted`).
  * Proibição de sufixos ou prefixos semânticos vazios (`Manager`, `Wrapper`, `Helper`).
  * Acrônimos de 2 letras: ambas em maiúsculas (`IOStream`, `IPAddress`). Acrônimos de 3 ou mais letras: apenas a primeira letra em maiúscula (`XmlParser`, `HttpConnection`, `JsonSerializer`).

---

### 2.3 Formatação e Layout de Código

* **Indentação:**
  * Exatamente 4 espaços. Proibição absoluta do uso de caracteres de tabulação (`\t`).
* **Chaves e Quebras de Linha:**
  * Chaves de estilo Java/Kotlin: abertura no final da linha do construto, fechamento em linha separada alinhada horizontalmente com o início do bloco.
* **Espaçamento Horizontal (Whitespace):**
  * Espaço ao redor de operadores binários (`a + b`, `x * y`), **exceto** o operador de range (`0..n`, `0..<n`).
  * Sem espaços ao redor de operadores unários (`count++`, `-value`).
  * Espaço entre palavras-chave de fluxo (`if`, `when`, `for`, `while`) e o parêntese de abertura.
  * Sem espaço antes do parêntese de abertura em construtores primários, declarações de métodos e chamadas de métodos (`class Container(val id: String)`, `fun run()`, `calculate(10)`).
  * Sem espaços imediatamente após `(`, `[` ou antes de `]`, `)`.
  * Sem espaços ao redor de `.` ou `?.`.
  * Sem espaços ao redor dos colchetes angulares de tipos genéricos (`List<String>`).
  * Sem espaços ao redor do operador de referência a membros `::` (`String::length`).
  * Sem espaço antes de `?` marcador de nulabilidade (`String?`).
  * Sem alinhamento horizontal artificial de colunas de texto.
* **Uso do Dois-Pontos (`:`):**
  * Espaço antes de `:` apenas quando separa tipo de supertipo (`class MyClass : SuperClass`), em delegação de construtor (`: this(...)`), ou após palavra-chave `object`.
  * Sem espaço antes de `:` quando separa uma declaração de seu tipo (`val id: String`, `fun count(): Int`).
  * Sempre incluir espaço após `:`.
* **Cabeçalhos de Classe com Parâmetros Multilinhas:**
  * Construtores com múltiplos parâmetros devem quebrar cada parâmetro em uma linha separada com indentação de 4 espaços, com o parêntese de fechamento em nova linha:
    ```kotlin
    class UserProfile(
        val identifier: UserId,
        val emailAddress: EmailAddress,
        val displayName: String,
    ) : Entity<UserId>(identifier)
    ```
* **Ordem Canônica Oficial de Modificadores (16 posições estritas):**
  1. `public` / `protected` / `private` / `internal`
  2. `expect` / `actual`
  3. `final` / `open` / `abstract` / `sealed` / `const`
  4. `external`
  5. `override`
  6. `lateinit`
  7. `tailrec`
  8. `vararg`
  9. `suspend`
  10. `inner`
  11. `enum` / `annotation` / `fun` (em `fun interface`)
  12. `companion`
  13. `inline` / `value`
  14. `infix`
  15. `operator`
  16. `data`
* **Anotações:**
  * Posicionadas antes dos modificadores (`@Named("Default") private val service: Service`).
  * Anotações com argumentos ficam em linhas separadas. Anotações sem argumentos podem ser agrupadas na mesma linha.
  * File annotations (`@file:JvmName(...)`) são posicionadas antes do comando `package` e separadas por linha em branco.
* **Corpo de Expressão (Expression Body):**
  * Preferir expression body para funções com retorno de expressão única (`fun isEmpty() = size == 0`).
  * Se a expressão quebrar linha, coloque o operador `=` na primeira linha e indente a expressão por 4 espaços.
* **Propriedades Read-Only Simples:**
  * Em linha única quando triviais: `val isComplete: Boolean get() = remainingCount == 0`.
* **Estruturas de Controle Multilinhas:**
  * Condições multilinhas em `if` ou `when` fecham o parêntese junto com a chave de abertura em linha dedicada:
    ```kotlin
    if (isValid &&
        isProcessed
    ) {
        dispatch()
    }
    ```
  * As palavras-chave `else`, `catch`, `finally` e o `while` de loops `do-while` residem na mesma linha da chave de fechamento precedente (`} else {`).
* **Encadeamento de Chamadas (Chained Call Wrapping):**
  * Ao quebrar chamadas encadeadas, posicione o ponto `.` ou operador safe-call `?.` na linha seguinte com indentação única de 4 espaços:
    ```kotlin
    val activeNames = items
        .filter { it.isActive }
        .map { it.name }
    ```
* **Lambdas e Parâmetros:**
  * Espaço dentro das chaves `{ ... }` e ao redor da seta `->`.
  * Quando a função receber uma única lambda, passe-a fora dos parênteses.
  * Em lambdas multilinhas com parâmetros, posicione os parâmetros na primeira linha seguidos por `->` e quebra de linha.
* **Trailing Commas Obrigatórias/Encorajadas:**
  * Use vírgula final (trailing comma) em todos os elementos multilinhas: parâmetros de construtores, argumentos de funções, entradas de `when`, enums, desestruturação e tipos genéricos, garantindo diffs limpos e consistentes.
* **Omissão de Elementos Sintáticos Redundantes:**
  * Omitir `: Unit` em declarações de retorno de funções.
  * Omitir ponto-e-vírgula (`;`).
  * Omitir chaves em interpolação de strings para variáveis simples (`"$userName"` e não `"${userName}"`).
  * Usar interpolação multi-dollar `$$"""..."""` quando o texto contiver caracteres `$` como literais.

---

## ⚡ 3. Idiomas Kotlin, Null Safety & Concorrência Estruturada

### 3.1 Imutabilidade e Idiomas Centrais da Linguagem

* **`val` Estrito sobre `var`:**
  * Declare invariavelmente propriedades e variáveis locais como `val`. O uso de `var` é restrito a contadores locais em loops de altíssimo desempenho ou buffers mutáveis encapsulados privadamente.
* **Coleções Somente-Leitura:**
  * Utilize sempre interfaces imutáveis de coleção (`List`, `Set`, `Map`) em contratos públicos, parâmetros e retornos.
  * Utilize factory functions imutáveis (`listOf()`, `setOf()`, `mapOf()`) em vez de tipos mutáveis concretos (`ArrayList`, `HashSet`).
* **Expressões sobre Declarações:**
  * Trate `if`, `when` e `try` como expressões funcionais:
    ```kotlin
    val statusLabel = if (isActive) "ONLINE" else "OFFLINE"
    val parsedValue = try { input.toInt() } catch (e: NumberFormatException) { 0 }
    ```
  * Use `if` para ramificações puramente binárias e `when` para 3 ou mais condições.
  * Guard conditions em ramos de `when`: utilize parênteses para agrupar expressões booleanas compostas (`is Status.Success if (status.code == 200) -> handleSuccess()`).
* **Loop em Ranges com Limite Superior Exclusivo:**
  * Utilize estritamente o operador `..<` em substituição ao legado `0..n - 1`:
    ```kotlin
    for (index in 0..<items.size) {
        processItem(items[index])
    }
    ```
* **Argumentos Nomeados e Parâmetros com Valores Padrão:**
  * Prefira parâmetros com valores padrão a sobrecargas artificiais de funções.
  * Utilize argumentos nomeados ao passar múltiplos valores de tipos primitivos idênticos ou booleanos (`calculateArea(width = 10, height = 20, isScaleApplied = false)`).
* **Funções de Escopo (`let`, `run`, `with`, `apply`, `also`):**
  * `apply`: Configuração e inicialização de propriedades do próprio receptor (retorna o próprio receptor).
  * `also`: Ações laterais e logs que não alteram o contexto do pipeline (retorna o próprio receptor).
  * `let`: Conversão de dados e execução condicionada a não-nulidade via safe-call `?.let { ... }`.
  * `run`: Computação com escopo do receptor retornando o resultado da expressão final.
  * `with`: Execução de múltiplos membros em um objeto sem valor de retorno encadeado.
* **Modelagem Algébrica com Sealed Interfaces & Classes:**
  * Modele estados de domínio, hierarquias de erro e eventos como `sealed interface` ou `sealed class`, garantindo exaustividade em expressões `when` sem necessidade de ramos `else` artificiais.
* **Newtypes e Zero-Cost Abstractions (`@JvmInline value class`):**
  * Modele identificadores e valores de domínio fortemente tipados usando `@JvmInline value class` para garantir type-safety em tempo de compilação sem custo de alocação no runtime:
    ```kotlin
    @JvmInline
    value class CustomerId(val value: String)
    ```

---

### 3.2 Null Safety Rigorosa (Zero Double-Bang Invariant)

* **Proibição Absoluta do Operador `!!`:**
  * É terminantemente proibido o uso do operador double-bang (`!!`) em qualquer código de produção.
* **Safe Calls e Operador Elvis (`?:`):**
  * Trate nulabilidade defensivamente com safe calls (`?.`), operador Elvis associado a retorno antecipado (`?: return`, `?: return null`) ou lançamento de exceções semânticas de domínio (`?: throw InvalidEntityStateException(...)`).
* **Booleans Anuláveis:**
  * Ao avaliar expressões booleanas anuláveis em condicionais, use checagens explícitas: `if (flag == true)`.
* **Smart Casts:**
  * Aproveite os smart casts nativos do Kotlin após verificações de tipo (`is`) ou nulabilidade (`!= null`), evitando casts explícitos desnecessários (`as`).

---

### 3.3 Coroutines & Concorrência Estruturada

* **Proibição Absoluta de `GlobalScope`:**
  * É expressamente proibido instanciar coroutines via `GlobalScope`. Concorrência deve ser estritamente vinculada a um `CoroutineScope` com ciclo de vida gerenciado.
* **Concorrência Estruturada com `coroutineScope` e `supervisorScope`:**
  * Utilize `coroutineScope { ... }` para decompor tarefas paralelas onde a falha de um filho cancela os demais, ou `supervisorScope { ... }` quando a falha de um filho não deve comprometer a execução paralela de irmãos.
* **Cancelamento Cooperativo:**
  * Loops intensivos em coroutines devem checar ativamente por cancelamento chamando `ensureActive()` ou `yield()`.
  * Nunca capture e suprima `CancellationException` de forma silenciosa; sempre repropague para manter a árvore de cancelamento íntegra.
* **Isolamento de Dispatchers:**
  * `Dispatchers.Default`: Exclusivo para processamento intensivo de CPU, transformações de coleções pesadas e algoritmos criptográficos.
  * `Dispatchers.IO`: Exclusivo para operações bloqueantes de I/O em rede, sockets, sistemas de arquivos e JDBC tradicional.
* **Fluxos Reativos com `Flow`:**
  * Utilize `Flow<T>` assíncrono para streams contínuos de dados com suporte nativo a backpressure e cancelamento.
  * Exponha estados atômicos de concorrência através de `StateFlow<T>` imutável.

---

## 🚀 4. Extrema Performance & Volumetria (Latência Mínima)

* **Complexidade Algorítmica Ótima:**
  * Escolha rigorosa de algoritmos e estruturas de dados ($O(1)$, $O(\log n)$ ou $O(n)$ onde aplicável). Evite complexidade quadrática ou varreduras repetidas em coleções.
* **Zero Alocações Inúteis no Hot-Path:**
  * Minimize alocações de memória, clonagens profundas e cópias de buffers em caminhos críticos de execução.
  * Utilize `@JvmInline value class` para abstrações de custo zero, primitivos desempacotados sem boxing desnecessário e reaproveitamento de buffers.
* **I/O Assíncrono e Não-Bloqueante:**
  * Todo I/O de rede, disco ou IPC deve ser estritamente assíncrono e não-bloqueante (Coroutines, pools bem dimensionados). Jamais bloqueie threads primárias.
* **Streaming e Paginação Compulsórios:**
  * É expressamente proibido carregar massas de dados completas na memória RAM. Em leituras de arquivos grandes, respostas HTTP volumosas ou queries de banco, utilize invariavelmente `Sequence`, `Flow` ou paginação/chunking com limites estritos.
* **Queries & Payloads Enxutos:**
  * Selecione apenas as colunas/campos necessários. Evite over-fetching, payloads redundantes e serializações desnecessárias.
* **Validação Empírica de Hot-Paths:**
  * Protocolo de micro-benchmarking e profiling com `kotlin.system.measureNanoTime` ou a biblioteca oficial `kotlinx-benchmark` em algoritmos críticos, parsers e pipelines volumosos.

---

## 🚫 5. Proibição Absoluta de Comentários e Docstrings (Zero Comments Policy)

O código deve ser autodescritivo por excelência:

* **Proibição Absoluta:**
  * É expressamente proibido escrever qualquer comentário (`//`, `/* ... */`) ou docstring (KDoc `/** ... */`) dentro de arquivos de código-fonte.
* **Código Autodocumentado:**
  * O código deve explicar sua finalidade e intenção por meio de:
    - Nomes de variáveis, constantes, funções, classes e métodos 100% semânticos, precisos e contextualizados.
    - Tipagem estrita e sistemas de tipos ricos (`@JvmInline value class`, sealed hierarchies, enums algébricos).
    - Funções curtas e com responsabilidade única que evidenciam o fluxo lógico instantaneamente.
* **Edição de Arquivos Existentes:**
  * Ao modificar qualquer arquivo existente, jamais introduza comentários ou docstrings. Sempre que tocar em uma função, classe ou bloco, **remova os comentários e docstrings existentes** daquela unidade de código, refatorando-a se necessário para que permaneça cristalina sem necessitar de anotações textuais.

---

## 🎯 6. Catálogo Canônico GoF / Refactoring Guru (Aplicação On-Demand)

Os 22 padrões de projeto da Gang of Four (GoF), conforme catalogados no *Refactoring Guru*, servem de referência universal para modelagem e design orientado a objetos:

### 6.1 Harmonia com SOLID, Ports & Adapters e KISS

* **Aplicação Estrita On-Demand (Anti-Over-Engineering / YAGNI):** Padrões de projeto existem para resolver problemas reais e recorrentes de acoplamento, variação algorítmica e instanciação complexa. É expressamente proibido forçar a introdução de padrões preventivos ou puramente cosméticos quando uma função pura, composição linear ou estrutura simples atende ao problema com clareza (KISS absoluto).
* **Integridade Hexagonal:**
  * No **Domínio Puro (Hexágono Central)**: Padrões comportamentais e estruturais (Strategy, State, Composite, Observer) operam exclusivamente com regras e abstrações de negócio puras, livres de tecnologias externas.
  * Nos **Adaptadores (Bordas / Driven & Driving Adapters)**: Padrões como Adapter, Facade, Decorator e Proxy atuam na mediação, encapsulamento e resiliência de infraestrutura externa (SDKs, APIs, bancos).
  * No **Composition Root**: Padrões criacionais (Abstract Factory, Factory Method, Builder) centralizam a instanciação e fiação de dependências, evitando espalhar lógicas de criação pelas camadas internas.
* **Autodescritividade Sem Comentários:** O padrão empregado deve ser evidente pelos nomes semânticos das interfaces e classes (ex: `PaymentStrategy`, `S3StorageAdapter`, `UserQueryProxy`, `DocumentBuilder`), sem nenhum comentário explicativo no código.

### 6.2 Os 22 Padrões Catalogados

1. **Padrões Criacionais (5 padrões):**
   * **Factory Method:** Interface para criação de objetos, permitindo que subclasses ou implementações concretas decidam a classe exata a instanciar.
   * **Abstract Factory:** Cria famílias de objetos relacionados sem especificar suas classes concretas. Essencial em Composition Roots e fixtures de teste.
   * **Builder:** Constrói objetos complexos em passos graduais. Mandatório para DTOs e Value Objects com múltiplos parâmetros opcionais ou validações graduais.
   * **Prototype:** Clona instâncias existentes sem acoplar o código a classes concretas.
   * **Singleton (Gerenciado via DI):** Instância única com acesso controlado. *Restrição Mandatória:* Proibido singleton estático com estado mutável global. Utilize escopo Singleton gerenciado pelo container de DI ou Composition Root.

2. **Padrões Estruturais (7 padrões):**
   * **Adapter:** Converte a interface de uma classe na esperada pelo cliente. Pilar canônico de Ports & Adapters: todo Driven Adapter atua formalmente como um Adapter GoF.
   * **Bridge:** Desacopla uma abstração de sua implementação para que ambas variem independentemente.
   * **Composite:** Compõe objetos em estruturas de árvore todo-parte (ASTs, documentos, regras compostas).
   * **Decorator:** Agrega responsabilidades dinamicamente (observabilidade, caching, retries, rate-limiting) sem usar herança.
   * **Facade:** Fornece interface simplificada e unificada para um subsistema ou biblioteca complexa.
   * **Flyweight:** Compartilha estado intrínseco imutável para minimizar consumo de memória em coleções massivas.
   * **Proxy:** Controla acesso a outro objeto (Lazy Loading, proteção/permissões, caching, chamadas remotas).

3. **Padrões Comportamentais (10 padrões):**
   * **Chain of Responsibility:** Passa requisições ao longo de uma corrente de manipuladores (pipelines de validação defensiva, middlewares).
   * **Command:** Encapsula requisições como objetos autônomos (tarefas assíncronas, operações reversíveis).
   * **Iterator:** Acessa elementos de uma coleção sequencialmente sem expor sua representação interna (streaming com memória $O(1)$).
   * **Mediator:** Desacopla comunicação direta entre objetos centralizando a colaboração.
   * **Memento:** Captura e restaura o estado interno de um objeto sem violar o encapsulamento.
   * **Observer:** Notifica múltiplos observadores sobre mudanças de estado (Domain Events, fluxos reativos).
   * **State:** Altera o comportamento do objeto quando seu estado interno muda (máquinas de estados finitas).
   * **Strategy:** Família de algoritmos intercambiáveis em tempo de execução (elimina cadeias de `if/else`).
   * **Template Method:** Esqueleto de algoritmo com etapas customizáveis em subclasses (priorizar Strategy quando herança for rígida).
   * **Visitor:** Executa operações em nós de estruturas compostas sem modificar as classes desses nós.

---

## 🧪 7. Testes Rigorosos & Matriz Exaustiva (Exhaustive Testing Matrix)

### 7.1 Os 3 Níveis Mandatórios de Teste

1. **Testes Unitários:** Isolamento estrito da unidade sob teste (classes de domínio, use cases, models, value objects, pure functions). Todas as dependências secundárias (Driven Ports) são obrigatoriamente isoladas via test doubles (mocks/stubs com MockK), garantindo feedback em milissegundos e zero dependência de infraestrutura externa.
2. **Testes de Componente:** Validação integrada da coesão interna entre submódulos e camadas de um mesmo componente ou subsistema sem mocks internos (mocks são restritos a fronteiras externas de infraestrutura pesada).
3. **Testes de Integração:** Validação real de adaptadores externos (Driven Adapters) contra recursos de infraestrutura reais ou emulados fielmente (bancos de dados via Testcontainers, filesystem real em diretórios temporários, servidores HTTP locais de teste).

### 7.2 Matriz Exaustiva de Estados, Branches e Valores Limite

É expressamente proibido testar apenas o "happy path". O engenheiro/agente deve mapear e cobrir 100% das branches de todas as dependências e o espaço de estados:

* **Nulabilidade:** `null`, não-nulo (com dados parciais e completos).
* **Booleanos:** `null` (quando o tipo for anulável), `false`, `true`.
* **Comparações e Limites Numéricos (Boundary Value Analysis — BVA):**
  - `VALOR - 1` (imediatamente abaixo do limiar)
  - `VALOR` (exatamente no limiar)
  - `VALOR + 1` (imediatamente acima do limiar)
  - Extremos de domínio: `0`, `-1`, valores negativos, `MIN_VALUE`, `MAX_VALUE`, coleções vazias e coleções unitárias.
* **Enums, Estados e Variantes Seladas:** Toda variante possível retornada por dependências ou recebida como entrada deve possuir ao menos um cenário de teste dedicado.

### 7.3 Ordem Obrigatória de Implementação e Execução (Failure-First)

A suíte de testes deve ser estruturada e ordenada cronologicamente seguindo o princípio da precedência defensiva (da falha catastrófica ao sucesso pleno):

1. **1º Casos que causam Exceções / Falhas / Erros de Dependência:** Simulação de interrupções, exceptions lançadas por portas (`throws`, timeouts, erros de conexão, I/O failures).
2. **2º Casos que resultam em Falso / Nulo / Vazio / Early Return:** Branches de guarda (guard clauses), retornos negativos imediatos, dependências retornando `null`, entidades não encontradas ou flags inativas.
3. **3º Branches Intermediárias e Transições de Limites:** Avaliação de valores limítrofes (`VALOR - 1`, `VALOR`, `VALOR + 1`), limites de cota/saldo/permissão e variantes intermediárias de enums/estados.
4. **4º Casos de Sucesso Pleno (Happy Path / Full Success):** Fluxo principal onde todas as pré-condições são satisfeitas, valores estão dentro das faixas ótimas e o resultado esperado é retornado integralmente.

---

## 💻 8. Exemplo Canônico Completo de Referência (Produção)

Demonstração prática completa em Kotlin moderno combinando: Domínio Puro, Value Classes, Sealed Interfaces, Ports & Adapters, Coroutines Estruturadas, MockK, Matriz Exaustiva BVA, Ordem Failure-First e Zero Comentários.

### 8.1 Modelos e Contratos de Domínio Puro (`domain/`)

```kotlin
package org.example.kotlin.domain.model

@JvmInline
value class AccountId(val value: String)

@JvmInline
value class Money(val amountCents: Long) {
    init {
        require(amountCents >= 0)
    }

    operator fun plus(other: Money): Money = Money(amountCents + other.amountCents)
    operator fun minus(other: Money): Money {
        check(amountCents >= other.amountCents)
        return Money(amountCents - other.amountCents)
    }
}

enum class AccountStatus {
    ACTIVE,
    BLOCKED,
    PENDING_VERIFICATION,
}

data class BankAccount(
    val id: AccountId,
    val status: AccountStatus,
    val balance: Money,
)

sealed interface TransferResult {
    data class Success(val transactionId: String, val newBalance: Money) : TransferResult
    sealed interface Failure : TransferResult {
        data object AccountNotFound : Failure
        data object AccountInactive : Failure
        data object InsufficientFunds : Failure
    }
}
```

```kotlin
package org.example.kotlin.domain.port

import org.example.kotlin.domain.model.AccountId
import org.example.kotlin.domain.model.BankAccount
import org.example.kotlin.domain.model.Money

fun interface AccountPersistencePort {
    suspend fun findAccountById(id: AccountId): BankAccount?
}

fun interface AccountTransactionPort {
    suspend fun updateBalance(id: AccountId, newBalance: Money): String
}
```

```kotlin
package org.example.kotlin.domain.usecase

import org.example.kotlin.domain.model.AccountId
import org.example.kotlin.domain.model.AccountStatus
import org.example.kotlin.domain.model.Money
import org.example.kotlin.domain.model.TransferResult
import org.example.kotlin.domain.port.AccountPersistencePort
import org.example.kotlin.domain.port.AccountTransactionPort

class TransferFundsUseCase(
    private val persistencePort: AccountPersistencePort,
    private val transactionPort: AccountTransactionPort,
) {
    suspend operator fun invoke(
        sourceAccountId: AccountId,
        transferAmount: Money,
    ): TransferResult {
        val account = persistencePort.findAccountById(sourceAccountId)
            ?: return TransferResult.Failure.AccountNotFound
        if (account.status != AccountStatus.ACTIVE) {
            return TransferResult.Failure.AccountInactive
        }
        if (account.balance.amountCents < transferAmount.amountCents) {
            return TransferResult.Failure.InsufficientFunds
        }
        val updatedBalance = account.balance - transferAmount
        val transactionReference = transactionPort.updateBalance(sourceAccountId, updatedBalance)
        return TransferResult.Success(
            transactionId = transactionReference,
            newBalance = updatedBalance,
        )
    }
}
```

---

### 8.2 Suíte Exaustiva de Testes Unitários (JUnit 5 + MockK)

```kotlin
package org.example.kotlin.domain.usecase

import io.mockk.coEvery
import io.mockk.mockk
import kotlinx.coroutines.runBlocking
import org.example.kotlin.domain.model.AccountId
import org.example.kotlin.domain.model.AccountStatus
import org.example.kotlin.domain.model.BankAccount
import org.example.kotlin.domain.model.Money
import org.example.kotlin.domain.model.TransferResult
import org.example.kotlin.domain.port.AccountPersistencePort
import org.example.kotlin.domain.port.AccountTransactionPort
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.assertThrows

class TransferFundsUseCaseTest {
    private val persistencePort: AccountPersistencePort = mockk()
    private val transactionPort: AccountTransactionPort = mockk()
    private val useCase = TransferFundsUseCase(persistencePort, transactionPort)

    private val targetAccountId = AccountId("acc-1234")

    @Test
    fun `invoke when persistence throws exception propagates failure`() = runBlocking {
        coEvery { persistencePort.findAccountById(targetAccountId) } throws IllegalStateException("Database unreachable")

        assertThrows<IllegalStateException> {
            useCase(targetAccountId, Money(100L))
        }
    }

    @Test
    fun `invoke when accountNotFound returns AccountNotFound failure`() = runBlocking {
        coEvery { persistencePort.findAccountById(targetAccountId) } returns null

        val result = useCase(targetAccountId, Money(100L))

        assertEquals(TransferResult.Failure.AccountNotFound, result)
    }

    @Test
    fun `invoke when accountIsPendingVerification returns AccountInactive failure`() = runBlocking {
        val account = BankAccount(targetAccountId, AccountStatus.PENDING_VERIFICATION, Money(500L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account

        val result = useCase(targetAccountId, Money(100L))

        assertEquals(TransferResult.Failure.AccountInactive, result)
    }

    @Test
    fun `invoke when accountIsBlocked returns AccountInactive failure`() = runBlocking {
        val account = BankAccount(targetAccountId, AccountStatus.BLOCKED, Money(500L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account

        val result = useCase(targetAccountId, Money(100L))

        assertEquals(TransferResult.Failure.AccountInactive, result)
    }

    @Test
    fun `invoke when balanceBelowBoundary returns InsufficientFunds failure`() = runBlocking {
        val transferAmount = Money(1000L)
        val account = BankAccount(targetAccountId, AccountStatus.ACTIVE, Money(999L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account

        val result = useCase(targetAccountId, transferAmount)

        assertEquals(TransferResult.Failure.InsufficientFunds, result)
    }

    @Test
    fun `invoke when balanceExactBoundary returns Success with zero balance`() = runBlocking {
        val transferAmount = Money(1000L)
        val account = BankAccount(targetAccountId, AccountStatus.ACTIVE, Money(1000L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account
        coEvery { transactionPort.updateBalance(targetAccountId, Money(0L)) } returns "tx-bound-1"

        val result = useCase(targetAccountId, transferAmount)

        val expected = TransferResult.Success("tx-bound-1", Money(0L))
        assertEquals(expected, result)
    }

    @Test
    fun `invoke when balanceAboveBoundary returns Success with remainder`() = runBlocking {
        val transferAmount = Money(1000L)
        val account = BankAccount(targetAccountId, AccountStatus.ACTIVE, Money(1001L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account
        coEvery { transactionPort.updateBalance(targetAccountId, Money(1L)) } returns "tx-bound-2"

        val result = useCase(targetAccountId, transferAmount)

        val expected = TransferResult.Success("tx-bound-2", Money(1L))
        assertEquals(expected, result)
    }

    @Test
    fun `invoke when fullPreconditionsMet returns Success full path`() = runBlocking {
        val transferAmount = Money(5000L)
        val account = BankAccount(targetAccountId, AccountStatus.ACTIVE, Money(20000L))
        coEvery { persistencePort.findAccountById(targetAccountId) } returns account
        coEvery { transactionPort.updateBalance(targetAccountId, Money(15000L)) } returns "tx-full-success"

        val result = useCase(targetAccountId, transferAmount)

        val expected = TransferResult.Success("tx-full-success", Money(15000L))
        assertEquals(expected, result)
    }
}
```

---

## 📋 9. Checklist de Qualidade Pré-Conclusão (Definition of Done)

Antes de finalizar qualquer implementação ou refatoração em Kotlin, execute a seguinte validação obrigatória:

1. [ ] **SOLID & Hexagonal:** O domínio puro (`domain/`) possui zero dependências de frameworks, ORMs ou bibliotecas web? Todas as dependências apontam para o centro (*inward*)?
2. [ ] **Convenções Oficiais da JetBrains:** A formatação possui rigorosamente 4 espaços (sem tabs)? Os modificadores respeitam a ordem canônica dos 16 itens? Foram usadas trailing commas em elementos multilinhas?
3. [ ] **Idiomas & Null Safety:** Variáveis e coleções são imutáveis (`val`, `List`) por padrão? O operador `!!` está 100% ausente? Expressões `if`, `when` e `try` foram tratadas como valores? Newtypes `@JvmInline value class` foram adotados para type-safety de custo zero?
4. [ ] **Concorrência Estruturada:** Coroutines estão vinculadas a um `CoroutineScope` gerenciado (zero `GlobalScope`)? `CancellationException` é devidamente repropagada? `Dispatchers.IO` e `Dispatchers.Default` estão isolados?
5. [ ] **Zero Comments Policy:** O código possui **zero comentários** (`//`, `/* ... */`) e **zero docstrings** (KDoc `/** ... */`)? Código existente tocado teve comentários e docstrings removidos?
6. [ ] **Matriz Exaustiva de Testes:** A suíte de testes cobre nulabilidade, booleanos, enums e valores limítrofes (BVA: `VALOR - 1`, `VALOR`, `VALOR + 1`, extremos)? Os cenários estão dispostos na ordem cronológica: Exceções $\rightarrow$ Guardas/Nulos $\rightarrow$ Limites $\rightarrow$ Sucesso Pleno?
7. [ ] **Build & Checks:** A suíte de testes e os linter/formatters executam com sucesso absoluto (`./gradlew test check`)?
