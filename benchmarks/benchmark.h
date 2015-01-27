#ifndef BENCHMARK
#define BENCHMARK

#include <celero/Celero.h>

#include <boost/format.hpp>

#include <humblelogging/api.h>

#include <db.h>
#include <query.h>
#include <annotationsearch.h>
#include <regexannosearch.h>
#include <operators/precedence.h>
#include <operators/inclusion.h>
#include <operators/dominance.h>
#include <operators/overlap.h>
#include <operators/pointing.h>
#include <wrapper.h>

HUMBLE_LOGGER(logger, "default");

using namespace annis;

template<bool optimized, char const* corpusName>
class CorpusFixture : public ::celero::TestFixture
{
public:
  CorpusFixture()
    : corpus(corpusName)
  {

  }

  virtual void setUp(int64_t experimentValue)
  {

    counter = 0;
  }

  virtual void tearDown()
  {
     HL_INFO(logger, (boost::format("result %1%") % counter).str());
  }

  void addOverride(ComponentType ctype, std::string layer, std::string name, std::string implementation)
  {
    overrideImpl.insert(
          std::pair<Component, std::string>(
    {ctype, layer, name}, implementation)
          );
  }

  DB initDB()
  {
    DB result(optimized);
    char* testDataEnv = std::getenv("ANNIS4_TEST_DATA");
    std::string dataDir("data");
    if(testDataEnv != NULL)
    {
      dataDir = testDataEnv;
    }
    result.load(dataDir + "/" + corpus);

    if(!optimized)
    {
      // manually convert all components to fallback implementation
      auto components = result.getAllComponents();
      for(auto c : components)
      {
        result.convertComponent(c, "fallback");
      }
    }
    else
    {
      // check if we want to override some implementations
      for(auto it=overrideImpl.begin(); it != overrideImpl.end(); it++)
      {

        std::cerr << "OVERRIDE called" << std::endl;
        result.convertComponent(it->first, it->second);
      }
    }

    return result;
  }

  DB& getDB()
  {
    static DB db = initDB();
    return db;
  }

  virtual ~CorpusFixture() {}

public:
  unsigned int counter;

private:
  const std::string corpus;
  std::map<Component, std::string> overrideImpl;
};



#endif // BENCHMARK

